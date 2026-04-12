//! Smoke tests for the ontology indexer.
//!
//! The tests require `DATABASE_URL` (or a fallback `TEST_DATABASE_URL`) to
//! point at a running Postgres+TimescaleDB instance. If neither env var is
//! set, each test prints a warning and passes trivially so the suite still
//! runs in constrained environments (CI without Docker, etc.).
//!
//! Run with:
//!   docker compose up -d
//!   cargo test -p lothal-ontology -- --ignored --nocapture
//!
//! The tests are gated behind `#[ignore]` because they mutate the database
//! schema: the `public` schema is dropped and recreated so migrations run
//! from scratch against a clean slate.

use std::env;

use chrono::Utc;
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use lothal_ontology::indexer::{close_link, emit_event, soft_delete_object, upsert_link, upsert_object};
use lothal_ontology::{Describe, EventSpec, LinkSpec, ObjectRef};

fn test_database_url() -> Option<String> {
    env::var("TEST_DATABASE_URL")
        .ok()
        .or_else(|| env::var("DATABASE_URL").ok())
}

async fn reset_schema(pool: &PgPool) -> Result<(), sqlx::Error> {
    sqlx::query("DROP SCHEMA IF EXISTS public CASCADE")
        .execute(pool)
        .await?;
    sqlx::query("CREATE SCHEMA public").execute(pool).await?;
    sqlx::query("CREATE EXTENSION IF NOT EXISTS timescaledb CASCADE")
        .execute(pool)
        .await?;
    Ok(())
}

async fn bootstrap_pool() -> Option<PgPool> {
    let url = match test_database_url() {
        Some(u) => u,
        None => {
            eprintln!("skipping indexer smoke: no DATABASE_URL or TEST_DATABASE_URL set");
            return None;
        }
    };
    let pool = PgPool::connect(&url).await.expect("connect to test DB");
    reset_schema(&pool).await.expect("reset schema");
    sqlx::migrate!("../../migrations")
        .run(&pool)
        .await
        .expect("run migrations");
    Some(pool)
}

/// Minimal domain type used to exercise `upsert_object`.
#[derive(Serialize)]
struct TestThing {
    id: Uuid,
    name: String,
    notes: String,
}

impl Describe for TestThing {
    const KIND: &'static str = "test_thing";

    fn id(&self) -> Uuid {
        self.id
    }

    fn site_id(&self) -> Option<Uuid> {
        None
    }

    fn display_name(&self) -> String {
        self.name.clone()
    }
}

#[tokio::test]
#[ignore = "requires a live TimescaleDB — run via `cargo test -- --ignored`"]
async fn indexer_smoke_end_to_end() {
    let Some(pool) = bootstrap_pool().await else {
        return;
    };

    let src = TestThing {
        id: Uuid::new_v4(),
        name: "Source Thing".into(),
        notes: "a source object".into(),
    };
    let dst = TestThing {
        id: Uuid::new_v4(),
        name: "Dest Thing".into(),
        notes: "a destination object".into(),
    };

    // --- upsert_object ---
    let mut tx = pool.begin().await.expect("begin tx");
    upsert_object(&mut tx, &src).await.expect("upsert src");
    upsert_object(&mut tx, &dst).await.expect("upsert dst");
    tx.commit().await.expect("commit");

    let (src_count,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM objects WHERE kind = $1 AND id = $2")
            .bind(TestThing::KIND)
            .bind(src.id)
            .fetch_one(&pool)
            .await
            .expect("count objects");
    assert_eq!(src_count, 1, "src object present");

    // --- upsert_object is idempotent (re-insert should not duplicate) ---
    let mut tx = pool.begin().await.expect("begin tx");
    let src_renamed = TestThing {
        id: src.id,
        name: "Source Thing Renamed".into(),
        notes: "updated notes".into(),
    };
    upsert_object(&mut tx, &src_renamed)
        .await
        .expect("upsert renamed src");
    tx.commit().await.expect("commit");
    let (display_name,): (String,) =
        sqlx::query_as("SELECT display_name FROM objects WHERE kind = $1 AND id = $2")
            .bind(TestThing::KIND)
            .bind(src.id)
            .fetch_one(&pool)
            .await
            .expect("fetch renamed");
    assert_eq!(display_name, "Source Thing Renamed");

    // --- upsert_link ---
    let mut tx = pool.begin().await.expect("begin tx");
    let link = LinkSpec::new(
        "links_to",
        ObjectRef::new(TestThing::KIND, src.id),
        ObjectRef::new(TestThing::KIND, dst.id),
    )
    .with_properties(serde_json::json!({"weight": 1}));
    let link_id = upsert_link(&mut tx, link.clone()).await.expect("upsert link");
    tx.commit().await.expect("commit");
    assert!(!link_id.is_nil(), "link id returned");

    // Re-upsert with new props should return the same id (upsert, not insert).
    let mut tx = pool.begin().await.expect("begin tx");
    let relink = LinkSpec::new(
        "links_to",
        ObjectRef::new(TestThing::KIND, src.id),
        ObjectRef::new(TestThing::KIND, dst.id),
    )
    .with_properties(serde_json::json!({"weight": 2}));
    let link_id_again = upsert_link(&mut tx, relink).await.expect("upsert link again");
    tx.commit().await.expect("commit");
    assert_eq!(
        link_id, link_id_again,
        "repeat upsert reuses the current link row"
    );

    // --- emit_event ---
    let mut tx = pool.begin().await.expect("begin tx");
    let ev = EventSpec {
        kind: "test_thing_touched".into(),
        site_id: None,
        subjects: vec![ObjectRef::new(TestThing::KIND, src.id)],
        summary: "touched src".into(),
        severity: Some("info".into()),
        properties: serde_json::json!({"reason": "smoke test"}),
        source: "indexer_smoke".into(),
    };
    let event_id = emit_event(&mut tx, ev).await.expect("emit event");
    tx.commit().await.expect("commit");
    assert!(!event_id.is_nil(), "event id returned");

    let (event_count,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM events WHERE id = $1")
            .bind(event_id)
            .fetch_one(&pool)
            .await
            .expect("count events");
    assert_eq!(event_count, 1, "event row inserted");

    // --- close_link ---
    let mut tx = pool.begin().await.expect("begin tx");
    close_link(
        &mut tx,
        "links_to",
        ObjectRef::new(TestThing::KIND, src.id),
        ObjectRef::new(TestThing::KIND, dst.id),
        Utc::now(),
    )
    .await
    .expect("close link");
    tx.commit().await.expect("commit");

    let (open_count,): (i64,) = sqlx::query_as(
        "SELECT count(*) FROM links WHERE kind = $1 AND src_id = $2 AND dst_id = $3 AND valid_until IS NULL",
    )
    .bind("links_to")
    .bind(src.id)
    .bind(dst.id)
    .fetch_one(&pool)
    .await
    .expect("count open links");
    assert_eq!(open_count, 0, "link closed");

    // --- soft_delete_object ---
    let mut tx = pool.begin().await.expect("begin tx");
    soft_delete_object(&mut tx, TestThing::KIND, src.id)
        .await
        .expect("soft delete");
    tx.commit().await.expect("commit");
    let (deleted_at,): (Option<chrono::DateTime<chrono::Utc>>,) =
        sqlx::query_as("SELECT deleted_at FROM objects WHERE kind = $1 AND id = $2")
            .bind(TestThing::KIND)
            .bind(src.id)
            .fetch_one(&pool)
            .await
            .expect("fetch deleted_at");
    assert!(deleted_at.is_some(), "deleted_at populated");
}
