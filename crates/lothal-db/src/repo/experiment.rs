use chrono::NaiveDate;
use sqlx::PgPool;
use uuid::Uuid;

use lothal_core::ontology::experiment::{
    Experiment, ExperimentStatus, Hypothesis, HypothesisCategory, Intervention, Recommendation,
};
use lothal_core::temporal::DateRange;
use lothal_core::units::Usd;
use lothal_ontology::indexer;
use lothal_ontology::{Describe, EventSpec, LinkSpec, ObjectRef};

// ---------------------------------------------------------------------------
// Hypothesis
// ---------------------------------------------------------------------------

pub async fn insert_hypothesis(pool: &PgPool, h: &Hypothesis) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"INSERT INTO hypotheses (id, site_id, title, description,
                                   expected_savings_pct, expected_savings_usd,
                                   category, created_at)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"#,
    )
    .bind(h.id)
    .bind(h.site_id)
    .bind(&h.title)
    .bind(&h.description)
    .bind(h.expected_savings_pct)
    .bind(h.expected_savings_usd.map(|u| u.value()))
    .bind(h.category.to_string())
    .bind(h.created_at)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list_hypotheses_by_site(
    pool: &PgPool,
    site_id: Uuid,
) -> Result<Vec<Hypothesis>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, site_id, title, description, expected_savings_pct,
                expected_savings_usd, category, created_at
         FROM hypotheses WHERE site_id = $1 ORDER BY created_at",
    )
    .bind(site_id)
    .fetch_all(pool)
    .await?;

    Ok(rows.iter().map(hypothesis_from_row).collect())
}

fn hypothesis_from_row(row: &sqlx::postgres::PgRow) -> Hypothesis {
    use sqlx::Row;
    let cat_str: String = row.get("category");
    let savings_usd: Option<f64> = row.get("expected_savings_usd");
    Hypothesis {
        id: row.get("id"),
        site_id: row.get("site_id"),
        title: row.get("title"),
        description: row.get("description"),
        expected_savings_pct: row.get("expected_savings_pct"),
        expected_savings_usd: savings_usd.map(Usd::new),
        category: parse_hypothesis_category(&cat_str),
        created_at: row.get("created_at"),
    }
}

// ---------------------------------------------------------------------------
// Intervention
// ---------------------------------------------------------------------------

pub async fn insert_intervention(pool: &PgPool, i: &Intervention) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"INSERT INTO interventions (id, site_id, device_id, description,
                                      date_applied, cost, reversible, created_at)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"#,
    )
    .bind(i.id)
    .bind(i.site_id)
    .bind(i.device_id)
    .bind(&i.description)
    .bind(i.date_applied)
    .bind(i.cost.map(|c| c.value()))
    .bind(i.reversible)
    .bind(i.created_at)
    .execute(pool)
    .await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Experiment
// ---------------------------------------------------------------------------

pub async fn insert_experiment(pool: &PgPool, e: &Experiment) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query(
        r#"INSERT INTO experiments
               (id, site_id, hypothesis_id, intervention_id,
                baseline_start, baseline_end, result_start, result_end,
                status, actual_savings_pct, actual_savings_usd,
                confidence, notes, created_at, updated_at)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)"#,
    )
    .bind(e.id)
    .bind(e.site_id)
    .bind(e.hypothesis_id)
    .bind(e.intervention_id)
    .bind(e.baseline_period.start)
    .bind(e.baseline_period.end)
    .bind(e.result_period.start)
    .bind(e.result_period.end)
    .bind(e.status.to_string())
    .bind(e.actual_savings_pct)
    .bind(e.actual_savings_usd.map(|u| u.value()))
    .bind(e.confidence)
    .bind(&e.notes)
    .bind(e.created_at)
    .bind(e.updated_at)
    .execute(&mut *tx)
    .await?;

    indexer::upsert_object(&mut tx, e).await?;
    // `Experiment` targets the site it runs in; finer-grained `targets` edges
    // to a device or circuit would have to be resolved via the intervention
    // row (which is not guaranteed to be inserted yet and has no Describe
    // impl of its own).
    indexer::upsert_link(
        &mut tx,
        LinkSpec::new(
            "targets",
            ObjectRef::new(Experiment::KIND, e.id),
            ObjectRef::new("site", e.site_id),
        ),
    )
    .await?;
    indexer::emit_event(&mut tx, EventSpec::record_registered(e, "repo:experiment")).await?;

    tx.commit().await?;
    Ok(())
}

pub async fn get_experiment(pool: &PgPool, id: Uuid) -> Result<Option<Experiment>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT id, site_id, hypothesis_id, intervention_id,
                baseline_start, baseline_end, result_start, result_end,
                status, actual_savings_pct, actual_savings_usd,
                confidence, notes, created_at, updated_at
         FROM experiments WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| experiment_from_row(&r)))
}

pub async fn list_experiments_by_site(
    pool: &PgPool,
    site_id: Uuid,
) -> Result<Vec<Experiment>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, site_id, hypothesis_id, intervention_id,
                baseline_start, baseline_end, result_start, result_end,
                status, actual_savings_pct, actual_savings_usd,
                confidence, notes, created_at, updated_at
         FROM experiments WHERE site_id = $1 ORDER BY created_at",
    )
    .bind(site_id)
    .fetch_all(pool)
    .await?;

    Ok(rows.iter().map(experiment_from_row).collect())
}

pub async fn update_experiment(pool: &PgPool, e: &Experiment) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query(
        r#"UPDATE experiments SET
               status = $2, actual_savings_pct = $3, actual_savings_usd = $4,
               confidence = $5, notes = $6, updated_at = $7
           WHERE id = $1"#,
    )
    .bind(e.id)
    .bind(e.status.to_string())
    .bind(e.actual_savings_pct)
    .bind(e.actual_savings_usd.map(|u| u.value()))
    .bind(e.confidence)
    .bind(&e.notes)
    .bind(e.updated_at)
    .execute(&mut *tx)
    .await?;

    indexer::upsert_object(&mut tx, e).await?;

    tx.commit().await?;
    Ok(())
}

fn experiment_from_row(row: &sqlx::postgres::PgRow) -> Experiment {
    use sqlx::Row;
    let status_str: String = row.get("status");
    let savings_usd: Option<f64> = row.get("actual_savings_usd");
    let baseline_start: NaiveDate = row.get("baseline_start");
    let baseline_end: NaiveDate = row.get("baseline_end");
    let result_start: NaiveDate = row.get("result_start");
    let result_end: NaiveDate = row.get("result_end");

    Experiment {
        id: row.get("id"),
        site_id: row.get("site_id"),
        hypothesis_id: row.get("hypothesis_id"),
        intervention_id: row.get("intervention_id"),
        baseline_period: DateRange::new(baseline_start, baseline_end),
        result_period: DateRange::new(result_start, result_end),
        status: parse_experiment_status(&status_str),
        actual_savings_pct: row.get("actual_savings_pct"),
        actual_savings_usd: savings_usd.map(Usd::new),
        confidence: row.get("confidence"),
        notes: row.get("notes"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

// ---------------------------------------------------------------------------
// Recommendation
// ---------------------------------------------------------------------------

pub async fn insert_recommendation(pool: &PgPool, r: &Recommendation) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"INSERT INTO recommendations
               (id, site_id, device_id, title, description, category,
                estimated_annual_savings, estimated_capex, payback_years,
                confidence, priority_score, data_requirements, created_at)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)"#,
    )
    .bind(r.id)
    .bind(r.site_id)
    .bind(r.device_id)
    .bind(&r.title)
    .bind(&r.description)
    .bind(r.category.to_string())
    .bind(r.estimated_annual_savings.value())
    .bind(r.estimated_capex.value())
    .bind(r.payback_years)
    .bind(r.confidence)
    .bind(r.priority_score)
    .bind(&r.data_requirements)
    .bind(r.created_at)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list_recommendations_by_site(
    pool: &PgPool,
    site_id: Uuid,
) -> Result<Vec<Recommendation>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, site_id, device_id, title, description, category,
                estimated_annual_savings, estimated_capex, payback_years,
                confidence, priority_score, data_requirements, created_at
         FROM recommendations WHERE site_id = $1 ORDER BY priority_score DESC",
    )
    .bind(site_id)
    .fetch_all(pool)
    .await?;

    Ok(rows.iter().map(recommendation_from_row).collect())
}

/// Fetch a single persisted recommendation by id.
///
/// Returns `Ok(None)` when the id does not exist. The `apply_recommendation`
/// action depends on this lookup; the engine's `generate_recommendations` does
/// not persist on its own, so callers must `insert_recommendation` before a
/// recommendation can be looked up here.
pub async fn get_recommendation(
    pool: &PgPool,
    id: Uuid,
) -> Result<Option<Recommendation>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT id, site_id, device_id, title, description, category,
                estimated_annual_savings, estimated_capex, payback_years,
                confidence, priority_score, data_requirements, created_at
         FROM recommendations WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| recommendation_from_row(&r)))
}

fn recommendation_from_row(row: &sqlx::postgres::PgRow) -> Recommendation {
    use sqlx::Row;
    let cat_str: String = row.get("category");
    Recommendation {
        id: row.get("id"),
        site_id: row.get("site_id"),
        device_id: row.get("device_id"),
        title: row.get("title"),
        description: row.get("description"),
        category: parse_hypothesis_category(&cat_str),
        estimated_annual_savings: Usd::new(row.get("estimated_annual_savings")),
        estimated_capex: Usd::new(row.get("estimated_capex")),
        payback_years: row.get("payback_years"),
        confidence: row.get("confidence"),
        priority_score: row.get("priority_score"),
        data_requirements: row.get("data_requirements"),
        created_at: row.get("created_at"),
    }
}

// ---------------------------------------------------------------------------
// Enum parsers (no FromStr on these types in lothal-core)
// ---------------------------------------------------------------------------

fn parse_hypothesis_category(s: &str) -> HypothesisCategory {
    match s.to_lowercase().as_str() {
        "device swap" => HypothesisCategory::DeviceSwap,
        "behavior change" => HypothesisCategory::BehaviorChange,
        "envelope upgrade" => HypothesisCategory::EnvelopeUpgrade,
        "rate optimization" => HypothesisCategory::RateOptimization,
        "load shifting" => HypothesisCategory::LoadShifting,
        "maintenance" => HypothesisCategory::Maintenance,
        _ => HypothesisCategory::Other,
    }
}

fn parse_experiment_status(s: &str) -> ExperimentStatus {
    match s.to_lowercase().as_str() {
        "planned" => ExperimentStatus::Planned,
        "active" => ExperimentStatus::Active,
        "completed" => ExperimentStatus::Completed,
        "inconclusive" => ExperimentStatus::Inconclusive,
        "cancelled" => ExperimentStatus::Cancelled,
        _ => ExperimentStatus::Active,
    }
}
