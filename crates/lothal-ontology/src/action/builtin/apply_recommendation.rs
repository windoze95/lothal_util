//! `apply_recommendation` — materialize a stored [`Recommendation`] into a
//! running [`Experiment`] with a [`Hypothesis`], an [`Intervention`], and a
//! snapshot of the current baseline usage.
//!
//! # Flow
//!
//! 1. Resolve the single `site` or `device` subject from `_subjects[0]`.
//! 2. Fetch the `recommendations` row by `recommendation_id`. Missing id →
//!    `ActionError::InvalidInput`.
//! 3. Insert a fresh `hypotheses` row mirroring the recommendation's title,
//!    description, category, and estimated annual savings.
//! 4. Insert an `interventions` row targeting the subject (`device_id` when the
//!    subject is a device, otherwise NULL). `reversible` defaults to true;
//!    `applied_at` = today.
//! 5. Insert an `experiments` row joining the hypothesis + intervention.
//!    Baseline period = last 30 days; result period = baseline end → +N days
//!    (N from `override_duration_days` or 30 by default).
//! 6. Capture a baseline snapshot (mean kWh/day from `readings`) and write it
//!    to the new `experiments.baseline_snapshot` JSONB column. For a site
//!    subject with no electric readings, the snapshot is skipped and the
//!    output field is null.
//! 7. Emit `experiment_started` and `recommendation_applied` events.
//!
//! Recommendations are persisted in the `recommendations` table today (see
//! `lothal-db::repo::experiment::insert_recommendation`), so the input is an
//! id. If/when recommendations move to an on-demand-only model, this action
//! would grow a second input shape that accepts the payload inline.

use async_trait::async_trait;
use chrono::{Duration, NaiveDate, Utc};
use serde_json::json;
use uuid::Uuid;

use crate::action::{Action, ActionCtx, ActionError};
use crate::{EventSpec, ObjectRef};

use super::subjects_from_input;

pub struct ApplyRecommendation;

/// Default baseline window. Mirrored into `override_duration_days` as an upper
/// hint when the caller doesn't pass one.
const BASELINE_DAYS: i64 = 30;
/// Default result window when `override_duration_days` is absent.
const RESULT_DAYS_DEFAULT: i64 = 30;

#[async_trait]
impl Action for ApplyRecommendation {
    fn name(&self) -> &'static str {
        "apply_recommendation"
    }

    fn description(&self) -> &'static str {
        "Apply a stored recommendation: create an experiment + intervention, \
         snapshot baseline usage, and emit tracking events."
    }

    fn applicable_kinds(&self) -> &'static [&'static str] {
        &["site", "device"]
    }

    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "required": ["recommendation_id"],
            "properties": {
                "recommendation_id": {"type": "string", "format": "uuid"},
                "override_duration_days": {
                    "type": "integer",
                    "minimum": 1,
                    "maximum": 365
                }
            }
        })
    }

    fn output_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "required": ["experiment_id", "intervention_id", "event_id"],
            "properties": {
                "experiment_id":         {"type": "string", "format": "uuid"},
                "intervention_id":       {"type": "string", "format": "uuid"},
                "baseline_snapshot_id":  {"type": ["string", "null"], "format": "uuid"},
                "event_id":              {"type": "string", "format": "uuid"}
            }
        })
    }

    async fn run(
        &self,
        ctx: &ActionCtx,
        input: serde_json::Value,
    ) -> Result<serde_json::Value, ActionError> {
        // 1. Subject.
        let subjects = subjects_from_input(&input)?;
        let subject = subjects
            .first()
            .ok_or_else(|| {
                ActionError::InvalidInput("apply_recommendation requires one subject".into())
            })?
            .clone();

        // 2. Input parse.
        let rec_id = input
            .get("recommendation_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ActionError::InvalidInput("recommendation_id is required".into()))?;
        let rec_id = Uuid::parse_str(rec_id)
            .map_err(|e| ActionError::InvalidInput(format!("recommendation_id parse: {e}")))?;
        let duration_days = input
            .get("override_duration_days")
            .and_then(|v| v.as_i64())
            .map(|d| d.clamp(1, 365))
            .unwrap_or(RESULT_DAYS_DEFAULT);

        // 3. Fetch recommendation.
        let rec = fetch_recommendation(&ctx.pool, rec_id)
            .await?
            .ok_or_else(|| {
                ActionError::InvalidInput(format!("recommendation {rec_id} not found"))
            })?;

        // Sanity: subject's site must match the recommendation's site. If the
        // subject is a device we look up its site via the structure.
        let subject_site_id = resolve_site_id(&ctx.pool, &subject).await?;
        if subject_site_id != rec.site_id {
            return Err(ActionError::InvalidInput(format!(
                "subject site ({subject_site_id}) does not match recommendation site ({})",
                rec.site_id
            )));
        }

        // Date windows. Baseline = [today-30d, today); result = [today, today+N).
        let today: NaiveDate = Utc::now().date_naive();
        let baseline_start = today - Duration::days(BASELINE_DAYS);
        let baseline_end = today;
        let result_start = today;
        let result_end = today + Duration::days(duration_days);

        // 4–6. All row inserts + snapshot write + events happen in one tx so a
        // partial failure doesn't leave dangling hypothesis/intervention rows.
        let mut tx = ctx.pool.begin().await?;

        // Hypothesis.
        let hypothesis_id = Uuid::new_v4();
        sqlx::query(
            r#"INSERT INTO hypotheses
                   (id, site_id, title, description,
                    expected_savings_pct, expected_savings_usd,
                    category, created_at)
               VALUES ($1, $2, $3, $4, NULL, $5, $6, now())"#,
        )
        .bind(hypothesis_id)
        .bind(rec.site_id)
        .bind(&rec.title)
        .bind(&rec.description)
        .bind(rec.estimated_annual_savings) // persisted as f64 in `expected_savings_usd`
        .bind(&rec.category)
        .execute(&mut *tx)
        .await?;

        // Intervention.
        let intervention_id = Uuid::new_v4();
        let intervention_device_id = match subject.kind.as_str() {
            "device" => Some(subject.id),
            _ => rec.device_id,
        };
        let intervention_desc = format!("Applied from recommendation {rec_id}");
        sqlx::query(
            r#"INSERT INTO interventions
                   (id, site_id, device_id, description,
                    date_applied, cost, reversible, created_at)
               VALUES ($1, $2, $3, $4, $5, NULL, true, now())"#,
        )
        .bind(intervention_id)
        .bind(rec.site_id)
        .bind(intervention_device_id)
        .bind(&intervention_desc)
        .bind(today)
        .execute(&mut *tx)
        .await?;

        // Experiment.
        let experiment_id = Uuid::new_v4();
        sqlx::query(
            r#"INSERT INTO experiments
                   (id, site_id, hypothesis_id, intervention_id,
                    baseline_start, baseline_end, result_start, result_end,
                    status, actual_savings_pct, actual_savings_usd,
                    confidence, notes, created_at, updated_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8,
                       'active', NULL, NULL, NULL, NULL, now(), now())"#,
        )
        .bind(experiment_id)
        .bind(rec.site_id)
        .bind(hypothesis_id)
        .bind(intervention_id)
        .bind(baseline_start)
        .bind(baseline_end)
        .bind(result_start)
        .bind(result_end)
        .execute(&mut *tx)
        .await?;

        // 6. Baseline snapshot. Compute mean kWh/day and write into the new
        // `baseline_snapshot` JSONB column. A null `snapshot_id` in the output
        // means the subject had no electric readings in the window.
        let snapshot = compute_baseline(&mut tx, &subject, baseline_start, baseline_end).await?;
        let baseline_snapshot_id = match &snapshot {
            Some(s) => {
                let snap_id = Uuid::new_v4();
                let blob = json!({
                    "id": snap_id,
                    "kind": "electric_kwh_daily_mean",
                    "subject": { "kind": subject.kind, "id": subject.id },
                    "window_start": baseline_start.to_string(),
                    "window_end": baseline_end.to_string(),
                    "reading_count": s.reading_count,
                    "mean_kwh_per_day": s.mean_kwh_per_day,
                });
                sqlx::query("UPDATE experiments SET baseline_snapshot = $1 WHERE id = $2")
                    .bind(sqlx::types::Json(blob))
                    .bind(experiment_id)
                    .execute(&mut *tx)
                    .await?;
                Some(snap_id)
            }
            None => {
                tracing::warn!(
                    subject.kind = %subject.kind,
                    subject.id = %subject.id,
                    "apply_recommendation: no electric readings in baseline window; skipping snapshot",
                );
                None
            }
        };

        // Mirror experiment into `objects` + link it to the site so graph
        // traversals reach it. Matches the pattern used by
        // `lothal-db::repo::experiment::insert_experiment`.
        sqlx::query(
            r#"INSERT INTO objects (kind, id, display_name, site_id, properties, updated_at)
               VALUES ('experiment', $1, $2, $3, $4, now())
               ON CONFLICT (kind, id) DO UPDATE SET
                   display_name = EXCLUDED.display_name,
                   site_id      = EXCLUDED.site_id,
                   properties   = EXCLUDED.properties,
                   updated_at   = now(),
                   deleted_at   = NULL"#,
        )
        .bind(experiment_id)
        .bind(format!(
            "Experiment [Active] {result_start} to {result_end}"
        ))
        .bind(rec.site_id)
        .bind(sqlx::types::Json(json!({
            "hypothesis_id": hypothesis_id,
            "intervention_id": intervention_id,
            "recommendation_id": rec_id,
        })))
        .execute(&mut *tx)
        .await?;

        // 7. Emit the two business events inside the same transaction so
        //    downstream watchers see them only when the writes succeeded.
        let subject_ref = subject.clone();
        let experiment_ref = ObjectRef::new("experiment", experiment_id);
        let intervention_ref = ObjectRef::new("intervention", intervention_id);
        let recommendation_ref = ObjectRef::new("recommendation", rec_id);

        let event_id = crate::indexer::emit_event(
            &mut tx,
            EventSpec {
                kind: "experiment_started".into(),
                site_id: Some(rec.site_id),
                subjects: vec![
                    experiment_ref.clone(),
                    intervention_ref.clone(),
                    subject_ref.clone(),
                ],
                summary: rec.title.clone(),
                severity: Some("info".into()),
                properties: json!({
                    "experiment_id": experiment_id,
                    "hypothesis_id": hypothesis_id,
                    "intervention_id": intervention_id,
                    "recommendation_id": rec_id,
                    "baseline_start": baseline_start.to_string(),
                    "baseline_end": baseline_end.to_string(),
                    "result_start": result_start.to_string(),
                    "result_end": result_end.to_string(),
                    "baseline_snapshot_id": baseline_snapshot_id,
                    "invoked_by": ctx.invoked_by,
                }),
                source: "action:apply_recommendation".into(),
            },
        )
        .await?;

        // Second event — attributes the experiment to the originating recommendation.
        let _rec_event_id = crate::indexer::emit_event(
            &mut tx,
            EventSpec {
                kind: "recommendation_applied".into(),
                site_id: Some(rec.site_id),
                subjects: vec![recommendation_ref, experiment_ref],
                summary: format!("Applied recommendation: {}", rec.title),
                severity: Some("info".into()),
                properties: json!({
                    "recommendation_id": rec_id,
                    "experiment_id": experiment_id,
                    "invoked_by": ctx.invoked_by,
                }),
                source: "action:apply_recommendation".into(),
            },
        )
        .await?;

        tx.commit().await?;

        Ok(json!({
            "experiment_id": experiment_id,
            "intervention_id": intervention_id,
            "baseline_snapshot_id": baseline_snapshot_id,
            "event_id": event_id,
        }))
    }
}

/// Minimal recommendation payload the action needs. We hit the DB directly
/// rather than taking a dependency on `lothal-db` (which depends on
/// `lothal-ontology`, so the arrow can't be reversed).
struct RecommendationRow {
    site_id: Uuid,
    device_id: Option<Uuid>,
    title: String,
    description: String,
    category: String,
    estimated_annual_savings: f64,
}

async fn fetch_recommendation(
    pool: &sqlx::PgPool,
    id: Uuid,
) -> Result<Option<RecommendationRow>, sqlx::Error> {
    let row: Option<(Uuid, Option<Uuid>, String, String, String, f64)> = sqlx::query_as(
        "SELECT site_id, device_id, title, description, category, estimated_annual_savings
         FROM recommendations WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(
        |(site_id, device_id, title, description, category, savings)| RecommendationRow {
            site_id,
            device_id,
            title,
            description,
            category,
            estimated_annual_savings: savings,
        },
    ))
}

/// Resolve the owning site for a subject so we can sanity-check it against the
/// recommendation. Devices are anchored to a `structure`, which is anchored to
/// a site; sites resolve to themselves.
async fn resolve_site_id(pool: &sqlx::PgPool, subject: &ObjectRef) -> Result<Uuid, ActionError> {
    match subject.kind.as_str() {
        "site" => Ok(subject.id),
        "device" => {
            let row: Option<(Uuid,)> = sqlx::query_as(
                "SELECT s.site_id
                 FROM devices d
                 JOIN structures s ON s.id = d.structure_id
                 WHERE d.id = $1",
            )
            .bind(subject.id)
            .fetch_optional(pool)
            .await?;
            row.map(|(site_id,)| site_id).ok_or_else(|| {
                ActionError::InvalidInput(format!("device {} not found", subject.id))
            })
        }
        other => Err(ActionError::NotApplicable(other.to_string())),
    }
}

/// Baseline captured at experiment creation time.
#[derive(Debug)]
struct Baseline {
    reading_count: i64,
    mean_kwh_per_day: f64,
}

/// Compute a mean-kWh/day baseline for the subject over `[start, end)`.
///
/// * `device` — sums `electric_kwh` readings directly for the device id.
/// * `site` — sums `electric_kwh` across every device attached to the site
///   (via `structures`). Returns `None` when the site has no devices or zero
///   readings in the window.
async fn compute_baseline(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    subject: &ObjectRef,
    start: NaiveDate,
    end: NaiveDate,
) -> Result<Option<Baseline>, sqlx::Error> {
    let days = (end - start).num_days().max(1) as f64;
    let start_ts = start
        .and_hms_opt(0, 0, 0)
        .expect("midnight is valid")
        .and_utc();
    let end_ts = end
        .and_hms_opt(0, 0, 0)
        .expect("midnight is valid")
        .and_utc();

    // `(total_kwh_opt, reading_count)`. A row is always returned by the
    // aggregates but `sum` can be NULL when there are no matches.
    let (total_opt, count): (Option<f64>, i64) = match subject.kind.as_str() {
        "device" => {
            sqlx::query_as(
                "SELECT sum(value), count(*)::bigint
             FROM readings
             WHERE source_type = 'device' AND source_id = $1
               AND kind = 'electric_kwh'
               AND time >= $2 AND time < $3",
            )
            .bind(subject.id)
            .bind(start_ts)
            .bind(end_ts)
            .fetch_one(&mut **tx)
            .await?
        }
        "site" => {
            sqlx::query_as(
                "SELECT sum(r.value), count(*)::bigint
             FROM readings r
             JOIN devices d
               ON d.id = r.source_id AND r.source_type = 'device'
             JOIN structures s
               ON s.id = d.structure_id
             WHERE s.site_id = $1
               AND r.kind = 'electric_kwh'
               AND r.time >= $2 AND r.time < $3",
            )
            .bind(subject.id)
            .bind(start_ts)
            .bind(end_ts)
            .fetch_one(&mut **tx)
            .await?
        }
        _ => return Ok(None),
    };

    match (total_opt, count) {
        (Some(total), n) if n > 0 => Ok(Some(Baseline {
            reading_count: n,
            mean_kwh_per_day: total / days,
        })),
        _ => Ok(None),
    }
}
