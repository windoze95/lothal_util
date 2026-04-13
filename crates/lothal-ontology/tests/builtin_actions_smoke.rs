//! Smoke tests for the built-in non-LLM actions (`record_observation`,
//! `schedule_maintenance`). Mirrors `action_smoke.rs`: gated on a live
//! Postgres+TimescaleDB pointed at by `DATABASE_URL` (or `TEST_DATABASE_URL`).
//! Skips cleanly with a warning when neither is set.
//!
//! The LLM-dependent actions are intentionally not exercised here — their
//! output depends on `LlmCompleter`, which tests would have to stub.
//!
//! Run with:
//!   docker compose up -d
//!   cargo test -p lothal-ontology --test builtin_actions_smoke -- --ignored --nocapture

use std::env;

use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use lothal_ontology::action::ActionRegistry;
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
            eprintln!("skipping builtin_actions_smoke: no DATABASE_URL or TEST_DATABASE_URL set");
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

/// Seed the minimum fixture: one Site + one PropertyZone. Returns the
/// (site_id, zone_id) tuple. Uses plain SQL so the test has no compile-time
/// dependency on the typed repos.
async fn seed_site_and_zone(pool: &PgPool) -> (Uuid, Uuid) {
    let site_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO sites (id, address, city, state, zip, latitude, longitude,
                              lot_size_acres, climate_zone, soil_type)
           VALUES ($1, '1 test ln', 'guthrie', 'ok', '73044',
                   35.878, -97.425, 1.0, '7a', 'sandy loam')"#,
    )
    .bind(site_id)
    .execute(pool)
    .await
    .expect("seed site");

    let zone_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO property_zones (id, site_id, name, kind, area_sqft,
                                       sun_exposure, slope, soil_type, drainage, notes)
           VALUES ($1, $2, 'north paddock', 'paddock', 2500.0,
                   'full_sun', 'flat', 'sandy loam', 'good', NULL)"#,
    )
    .bind(zone_id)
    .bind(site_id)
    .execute(pool)
    .await
    .expect("seed zone");

    // The ontology `objects` row isn't normally inserted by the raw SQL above
    // (the typed repo would take care of that), but the actions themselves
    // don't depend on the row existing — `record_observation` only writes an
    // event, and `schedule_maintenance` writes its own `maintenance_event`
    // ontology row. So we leave `objects` alone and verify end-to-end.

    (site_id, zone_id)
}

#[tokio::test]
#[ignore = "requires a live TimescaleDB — run via `cargo test -- --ignored`"]
async fn record_observation_persists_event_and_audit_row() {
    let Some(pool) = bootstrap_pool().await else {
        return;
    };
    let (_site, zone) = seed_site_and_zone(&pool).await;

    let registry = ActionRegistry::with_defaults(pool.clone());
    let run = registry
        .invoke(
            "record_observation",
            "test:user",
            pool.clone(),
            vec![ObjectRef::new("property_zone", zone)],
            json!({ "text": "smoke test", "severity": "notice" }),
        )
        .await
        .expect("invoke record_observation");

    assert_eq!(run.status, "succeeded");
    assert!(run.output.is_some(), "output populated");

    // The observation event should exist for this zone with matching text.
    let (event_count,): (i64,) = sqlx::query_as(
        r#"SELECT count(*) FROM events
           WHERE kind = 'observation'
             AND subjects @> $1
             AND properties->>'text' = 'smoke test'"#,
    )
    .bind(sqlx::types::Json(json!([{ "kind": "property_zone", "id": zone }])))
    .fetch_one(&pool)
    .await
    .expect("count observation events");
    assert_eq!(event_count, 1, "exactly one observation event written");

    // The registry also wrote the standard action_completed event.
    let (completed_count,): (i64,) = sqlx::query_as(
        r#"SELECT count(*) FROM events WHERE kind = 'action_completed'
           AND properties->>'action' = 'record_observation'"#,
    )
    .fetch_one(&pool)
    .await
    .expect("count completed events");
    assert_eq!(completed_count, 1, "action_completed event emitted");
}

#[tokio::test]
#[ignore = "requires a live TimescaleDB — run via `cargo test -- --ignored`"]
async fn ingest_bill_pdf_is_registered_and_rejects_garbage() {
    // We don't exercise the full LLM+pdftotext pipeline here (both require
    // external processes / API keys). Instead we verify:
    //   (a) the action is present in `with_defaults`,
    //   (b) it complains about bad input before reaching those externals.
    // An optional richer fixture test would require embedding a real PDF and
    // mocking `LlmCompleter`, which we skip until a fixture is checked in.
    let Some(pool) = bootstrap_pool().await else {
        return;
    };
    let (site_id, _zone) = seed_site_and_zone(&pool).await;

    // Seed a utility_account so the subject resolves.
    let account_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO utility_accounts (id, site_id, provider_name, utility_type,
                                         account_number, meter_id, is_active,
                                         created_at, updated_at)
           VALUES ($1, $2, 'OG&E', 'electric', NULL, NULL, true, now(), now())"#,
    )
    .bind(account_id)
    .bind(site_id)
    .execute(&pool)
    .await
    .expect("seed utility_account");

    let registry = ActionRegistry::with_defaults(pool.clone());
    assert!(
        registry.get("ingest_bill_pdf").is_some(),
        "ingest_bill_pdf must be in with_defaults"
    );
    assert!(
        registry.get("apply_recommendation").is_some(),
        "apply_recommendation must be in with_defaults"
    );

    // No LLM wired → expect Other("Claude client not configured").
    let err = registry
        .invoke(
            "ingest_bill_pdf",
            "test:user",
            pool.clone(),
            vec![ObjectRef::new("utility_account", account_id)],
            json!({ "pdf_base64": "ZGVmaW5pdGVseS1ub3QtYS1wZGY=" }),
        )
        .await
        .expect_err("action should error without an LLM");
    let msg = err.to_string();
    assert!(
        msg.to_lowercase().contains("claude"),
        "expected LLM-not-configured error, got: {msg}"
    );
}

#[tokio::test]
#[ignore = "requires a live TimescaleDB — run via `cargo test -- --ignored`"]
async fn schedule_maintenance_persists_event_and_row() {
    let Some(pool) = bootstrap_pool().await else {
        return;
    };
    let (_site, zone) = seed_site_and_zone(&pool).await;

    let registry = ActionRegistry::with_defaults(pool.clone());
    let run = registry
        .invoke(
            "schedule_maintenance",
            "test:user",
            pool.clone(),
            vec![ObjectRef::new("property_zone", zone)],
            json!({
                "event_type": "paddock rotation",
                "description": "move flock to north paddock"
            }),
        )
        .await
        .expect("invoke schedule_maintenance");

    assert_eq!(run.status, "succeeded");

    // One maintenance_events row targeting the zone.
    let (me_count,): (i64,) = sqlx::query_as(
        r#"SELECT count(*) FROM maintenance_events
           WHERE target_type = 'property_zone' AND target_id = $1
             AND event_type  = 'paddock rotation'"#,
    )
    .bind(zone)
    .fetch_one(&pool)
    .await
    .expect("count maintenance rows");
    assert_eq!(me_count, 1, "exactly one maintenance_events row written");

    // One maintenance_scheduled event subject to the zone.
    let (sched_ev_count,): (i64,) = sqlx::query_as(
        r#"SELECT count(*) FROM events
           WHERE kind = 'maintenance_scheduled'
             AND subjects @> $1"#,
    )
    .bind(sqlx::types::Json(json!([{ "kind": "property_zone", "id": zone }])))
    .fetch_one(&pool)
    .await
    .expect("count scheduled events");
    assert_eq!(sched_ev_count, 1, "maintenance_scheduled event emitted");

    // Audit row output carries the maintenance_event_ids list with one id.
    let out = run.output.as_ref().unwrap().0.clone();
    let ids = out.get("maintenance_event_ids").and_then(|v| v.as_array()).expect("ids array");
    assert_eq!(ids.len(), 1, "one id returned");
}
