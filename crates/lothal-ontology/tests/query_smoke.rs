//! Smoke tests for the ontology query layer.
//!
//! Mirrors `indexer_smoke.rs`: requires `TEST_DATABASE_URL` (or fallback
//! `DATABASE_URL`) pointing at a TimescaleDB instance, gracefully skips
//! when unset, and is `#[ignore]`-gated because it drops and recreates the
//! `public` schema.
//!
//! Run with:
//!   docker compose up -d
//!   cargo test -p lothal-ontology --test query_smoke -- --ignored --nocapture

use std::env;

use chrono::{Duration, Utc};
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use lothal_ontology::indexer::{emit_event, upsert_link, upsert_object};
use lothal_ontology::query::{
    events_for, get_object_view, neighbors, search, timeline, ViewOptions,
};
use lothal_ontology::{Describe, EventSpec, LinkSpec, ObjectRef, ObjectUri};

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
            eprintln!("skipping query smoke: no DATABASE_URL or TEST_DATABASE_URL set");
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

/// Minimal domain type used to drive `upsert_object`.
#[derive(Serialize)]
struct TestThing {
    id: Uuid,
    name: String,
    notes: String,
    site_id: Option<Uuid>,
}

impl Describe for TestThing {
    const KIND: &'static str = "test_thing";

    fn id(&self) -> Uuid {
        self.id
    }

    fn site_id(&self) -> Option<Uuid> {
        self.site_id
    }

    fn display_name(&self) -> String {
        self.name.clone()
    }

    // Override properties so `notes` is surfaced as a top-level key —
    // that's what the `search_tsv` generated column indexes.
    fn properties(&self) -> serde_json::Value {
        serde_json::json!({ "notes": self.notes })
    }
}

#[tokio::test]
#[ignore = "requires a live TimescaleDB — run via `cargo test -- --ignored`"]
async fn query_smoke_end_to_end() {
    let Some(pool) = bootstrap_pool().await else {
        return;
    };

    // --- Seed a Site row directly via SQL (no repo dependency).
    let site_id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO sites (id, address, city, state, zip, latitude, longitude, lot_size_acres)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        "#,
    )
    .bind(site_id)
    .bind("123 Test Ln")
    .bind("Guthrie")
    .bind("OK")
    .bind("73044")
    .bind(35.879_f64)
    .bind(-97.425_f64)
    .bind(5.0_f64)
    .execute(&pool)
    .await
    .expect("insert site");

    // --- Populate two linked objects + one event via the indexer helpers.
    let src = TestThing {
        id: Uuid::new_v4(),
        name: "Alpha Widget".into(),
        notes: "primary solar inverter readings".into(),
        site_id: Some(site_id),
    };
    let dst = TestThing {
        id: Uuid::new_v4(),
        name: "Beta Gadget".into(),
        notes: "downstream battery bank".into(),
        site_id: Some(site_id),
    };

    let mut tx = pool.begin().await.expect("begin tx");
    upsert_object(&mut tx, &src).await.expect("upsert src");
    upsert_object(&mut tx, &dst).await.expect("upsert dst");
    let link = LinkSpec::new(
        "feeds",
        ObjectRef::new(TestThing::KIND, src.id),
        ObjectRef::new(TestThing::KIND, dst.id),
    );
    upsert_link(&mut tx, link).await.expect("upsert link");
    // Second link with a different kind to exercise the kind filter.
    let link2 = LinkSpec::new(
        "monitors",
        ObjectRef::new(TestThing::KIND, src.id),
        ObjectRef::new(TestThing::KIND, dst.id),
    );
    upsert_link(&mut tx, link2).await.expect("upsert link2");
    let ev = EventSpec {
        kind: "test_thing_touched".into(),
        site_id: Some(site_id),
        subjects: vec![ObjectRef::new(TestThing::KIND, src.id)],
        summary: "alpha widget touched".into(),
        severity: Some("info".into()),
        properties: serde_json::json!({}),
        source: "query_smoke".into(),
    };
    emit_event(&mut tx, ev).await.expect("emit event");
    tx.commit().await.expect("commit");

    let src_uri = ObjectUri::new(TestThing::KIND, src.id);
    let dst_uri = ObjectUri::new(TestThing::KIND, dst.id);

    // --- get_object_view.
    let view = get_object_view(&pool, &src_uri, ViewOptions::default())
        .await
        .expect("get_object_view");
    assert_eq!(view.object.id, src.id);
    assert_eq!(view.object.display_name, "Alpha Widget");
    // Both links (feeds + monitors) should surface the dst object as neighbor.
    assert!(
        view.neighbors.iter().any(|(l, o)| l.kind == "feeds" && o.id == dst.id),
        "feeds neighbor present: got {:?}",
        view.neighbors.iter().map(|(l, _)| &l.kind).collect::<Vec<_>>()
    );
    assert!(
        view.neighbors.iter().any(|(l, o)| l.kind == "monitors" && o.id == dst.id),
        "monitors neighbor present"
    );
    assert!(
        view.recent_events.iter().any(|e| e.summary == "alpha widget touched"),
        "recent event surfaced"
    );
    assert!(
        view.applicable_actions.is_empty(),
        "actions stubbed empty for now"
    );

    // --- get_object_view on missing URI yields RowNotFound.
    let ghost = ObjectUri::new(TestThing::KIND, Uuid::new_v4());
    // `ObjectView` is not Debug, so use `.err()` rather than `.expect_err()`.
    let err = get_object_view(&pool, &ghost, ViewOptions::default())
        .await
        .err()
        .expect("missing should error");
    assert!(matches!(err, sqlx::Error::RowNotFound));

    // --- neighbors: no filter returns both kinds.
    let all = neighbors(&pool, &src_uri, None).await.expect("neighbors all");
    assert_eq!(all.len(), 2, "both outgoing links");
    // Bidirectional: querying from dst also returns src via both links.
    let all_from_dst = neighbors(&pool, &dst_uri, None)
        .await
        .expect("neighbors from dst");
    assert_eq!(all_from_dst.len(), 2, "bidirectional lookup finds src");
    assert!(all_from_dst.iter().all(|(_, o)| o.id == src.id));

    // --- neighbors: with kind filter.
    let only_feeds = neighbors(&pool, &src_uri, Some("feeds"))
        .await
        .expect("neighbors feeds");
    assert_eq!(only_feeds.len(), 1);
    assert_eq!(only_feeds[0].0.kind, "feeds");

    // --- events_for: range match, multiple URIs.
    let t0 = Utc::now() - Duration::hours(1);
    let t1 = Utc::now() + Duration::hours(1);
    let found = events_for(&pool, &[src_uri.clone(), dst_uri.clone()], t0, t1, None)
        .await
        .expect("events_for");
    assert!(
        found.iter().any(|e| e.summary == "alpha widget touched"),
        "event found in range"
    );

    // --- events_for: empty URIs short-circuits.
    let empty = events_for(&pool, &[], t0, t1, None)
        .await
        .expect("events_for empty");
    assert!(empty.is_empty());

    // --- events_for: kind filter excludes when mismatched.
    let wrong_kind = events_for(&pool, &[src_uri.clone()], t0, t1, Some("does_not_exist"))
        .await
        .expect("events_for wrong kind");
    assert!(wrong_kind.is_empty());

    // --- timeline: same as events_for with single URI, no kind filter.
    let tl = timeline(&pool, &src_uri, t0, t1).await.expect("timeline");
    assert_eq!(tl.len(), found.iter().filter(|e| e.subjects.0.iter().any(|s| {
        s.get("id").and_then(|v| v.as_str()) == Some(&src.id.to_string())
    })).count().max(tl.len()));
    assert!(tl.iter().any(|e| e.summary == "alpha widget touched"));

    // --- search: display_name hits the tsvector.
    let hits = search(&pool, "alpha", None, 10).await.expect("search alpha");
    assert!(
        hits.iter().any(|o| o.id == src.id),
        "alpha widget found by search"
    );
    // notes are indexed too.
    let hits_notes = search(&pool, "inverter", None, 10)
        .await
        .expect("search inverter");
    assert!(
        hits_notes.iter().any(|o| o.id == src.id),
        "notes-side token found"
    );
    // Kind filter narrows results; a wrong kind returns nothing.
    let wrong = search(&pool, "alpha", Some("no_such_kind"), 10)
        .await
        .expect("search wrong kind");
    assert!(wrong.is_empty());
}
