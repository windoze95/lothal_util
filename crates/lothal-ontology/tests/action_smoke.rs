//! Smoke tests for the action registry + audit log.
//!
//! Mirrors `indexer_smoke.rs`: requires `DATABASE_URL` (or `TEST_DATABASE_URL`)
//! to point at a running Postgres+TimescaleDB instance. Without one the tests
//! skip cleanly so the suite still runs in constrained environments.
//!
//! Run with:
//!   docker compose up -d
//!   cargo test -p lothal-ontology -- --ignored --nocapture

use std::env;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use lothal_ontology::action::{Action, ActionCtx, ActionError, ActionRegistry};
use lothal_ontology::ObjectRef;

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
            eprintln!("skipping action smoke: no DATABASE_URL or TEST_DATABASE_URL set");
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

/// A trivial action that echoes its input. Used to exercise the registry
/// invocation path end-to-end without any domain-specific side effects.
struct NoopAction;

#[async_trait]
impl Action for NoopAction {
    fn name(&self) -> &'static str {
        "noop"
    }

    fn description(&self) -> &'static str {
        "echoes input unchanged; only applicable to `site`"
    }

    fn applicable_kinds(&self) -> &'static [&'static str] {
        &["site"]
    }

    fn input_schema(&self) -> serde_json::Value {
        json!({ "type": "object" })
    }

    fn output_schema(&self) -> serde_json::Value {
        json!({ "type": "object" })
    }

    async fn run(
        &self,
        _ctx: &ActionCtx,
        input: serde_json::Value,
    ) -> Result<serde_json::Value, ActionError> {
        Ok(input)
    }
}

fn build_registry() -> ActionRegistry {
    let mut reg = ActionRegistry::new();
    reg.register(Arc::new(NoopAction));
    reg
}

#[tokio::test]
#[ignore = "requires a live TimescaleDB — run via `cargo test -- --ignored`"]
async fn invoke_succeeds_on_applicable_subject() {
    let Some(pool) = bootstrap_pool().await else {
        return;
    };
    let registry = build_registry();

    let site_id = Uuid::new_v4();
    let run = registry
        .invoke(
            "noop",
            "test:user",
            pool.clone(),
            vec![ObjectRef::new("site", site_id)],
            json!({"hello": "world"}),
        )
        .await
        .expect("invoke succeeds");

    assert_eq!(run.status, "succeeded");
    assert_eq!(run.action_name, "noop");
    assert_eq!(run.invoked_by, "test:user");
    assert!(run.finished_at.is_some(), "finished_at populated");
    assert!(run.output.is_some(), "output populated");
    let out = run.output.as_ref().unwrap();
    assert_eq!(out.0, json!({"hello": "world"}));

    // action_completed event pointing at the subject should exist.
    let (event_count,): (i64,) = sqlx::query_as(
        r#"
        SELECT count(*) FROM events
        WHERE kind = 'action_completed'
          AND subjects @> $1
        "#,
    )
    .bind(sqlx::types::Json(json!([
        {"kind": "site", "id": site_id}
    ])))
    .fetch_one(&pool)
    .await
    .expect("count events");
    assert_eq!(event_count, 1, "action_completed event emitted");
}

#[tokio::test]
#[ignore = "requires a live TimescaleDB — run via `cargo test -- --ignored`"]
async fn invoke_rejects_inapplicable_kind() {
    let Some(pool) = bootstrap_pool().await else {
        return;
    };
    let registry = build_registry();

    let err = registry
        .invoke(
            "noop",
            "test:user",
            pool.clone(),
            vec![ObjectRef::new("device", Uuid::new_v4())],
            json!({}),
        )
        .await
        .expect_err("expected NotApplicable");

    match err {
        ActionError::NotApplicable(kind) => assert_eq!(kind, "device"),
        other => panic!("expected NotApplicable, got {other:?}"),
    }

    // No audit row should have been inserted on an applicability failure.
    let (run_count,): (i64,) = sqlx::query_as("SELECT count(*) FROM action_runs")
        .fetch_one(&pool)
        .await
        .expect("count runs");
    assert_eq!(run_count, 0, "no audit row on applicability rejection");
}

#[tokio::test]
#[ignore = "requires a live TimescaleDB — run via `cargo test -- --ignored`"]
async fn invoke_rejects_unknown_action() {
    let Some(pool) = bootstrap_pool().await else {
        return;
    };
    let registry = build_registry();

    let err = registry
        .invoke(
            "missing",
            "test:user",
            pool.clone(),
            vec![ObjectRef::new("site", Uuid::new_v4())],
            json!({}),
        )
        .await
        .expect_err("expected Unknown");

    match err {
        ActionError::Unknown(name) => assert_eq!(name, "missing"),
        other => panic!("expected Unknown, got {other:?}"),
    }
}
