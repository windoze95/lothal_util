//! Gather the data needed to render a daily briefing.
//!
//! Cross-domain joins are delegated to `lothal_ontology::query`:
//! `get_object_view` walks the site + its neighbors in one composed query,
//! and `events_for` pulls anomaly / maintenance_scheduled events in explicit
//! time windows.
//!
//! Three inputs stay outside the ontology on purpose:
//!   * **Weather** — `weather_observations` is a TimescaleDB hypertable.
//!   * **Readings** — `readings_daily` is a continuous aggregate; time-series
//!     data is intentionally not indexed into `objects`.
//!   * **Baseline** — a computed value from `lothal-engine`, not a lookup.

use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use lothal_ontology::query::{self, ViewOptions};
use lothal_ontology::{EventRecord, ObjectUri};

use crate::AiError;

/// All the data needed to generate a daily briefing.
#[derive(Debug, Clone, Serialize)]
pub struct BriefingContext {
    pub date: NaiveDate,
    pub site_id: Uuid,
    pub weather: Option<WeatherSummary>,
    pub total_kwh: Option<f64>,
    pub estimated_cost: Option<f64>,
    pub baseline_comparison: Option<BaselineComparison>,
    pub circuit_anomalies: Vec<CircuitAnomaly>,
    pub maintenance_due: Vec<MaintenanceDue>,
    pub active_experiments: Vec<ActiveExperiment>,
    // --- Property operations context ---
    pub pool_status: Option<PoolDayStatus>,
    pub livestock_summary: Option<LivestockDaySummary>,
    pub septic_alert: Option<SepticAlert>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WeatherSummary {
    pub avg_temp_f: f64,
    pub min_temp_f: f64,
    pub max_temp_f: f64,
    pub cooling_degree_days: f64,
    pub heating_degree_days: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct BaselineComparison {
    pub predicted_kwh: f64,
    pub actual_kwh: f64,
    pub deviation_pct: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct CircuitAnomaly {
    pub circuit_label: String,
    pub actual_hours: f64,
    pub avg_hours: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct MaintenanceDue {
    pub description: String,
    pub due_date: NaiveDate,
}

#[derive(Debug, Clone, Serialize)]
pub struct ActiveExperiment {
    pub title: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PoolDayStatus {
    pub pool_name: String,
    pub pump_runtime_hours: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LivestockDaySummary {
    pub flock_name: String,
    pub eggs: f64,
    pub feed_lbs: f64,
    pub mortality: i32,
}

#[derive(Debug, Clone, Serialize)]
pub struct SepticAlert {
    pub days_until_pump: i64,
    pub is_overdue: bool,
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Gather all context data for a daily briefing.
///
/// One `get_object_view(site)` walk + two ranged `events_for` queries replace
/// the old bespoke per-domain joins. Pools, flocks, and experiments are
/// resolved by projecting the site's neighbor list onto typed repo lookups.
pub async fn gather_context(
    pool: &PgPool,
    site_id: Uuid,
    date: NaiveDate,
) -> Result<BriefingContext, AiError> {
    let site_uri = ObjectUri::new("site", site_id);

    let yesterday_start = day_start_utc(date);
    let today_end = day_end_utc(date);
    let today_start = day_start_utc(date + chrono::Duration::days(1));
    let today_end_plus = day_end_utc(date + chrono::Duration::days(1));

    // Ontology view: site + neighbors + recent events where site is a subject.
    let view = query::get_object_view(
        pool,
        &site_uri,
        ViewOptions {
            event_limit: 50,
            neighbor_depth: 2,
        },
    )
    .await?;

    // Ontology event windows.
    let anomaly_events =
        query::events_for(pool, &[site_uri.clone()], yesterday_start, today_end, Some("anomaly"))
            .await
            .unwrap_or_default();
    let scheduled_maintenance_events = query::events_for(
        pool,
        &[site_uri.clone()],
        today_start,
        today_end_plus,
        Some("maintenance_scheduled"),
    )
    .await
    .unwrap_or_default();

    // Weather + readings + readings-derived anomalies (outside the ontology).
    let (weather, total_kwh, circuit_anomalies) = tokio::try_join!(
        fetch_weather_summary(pool, site_id, date),
        fetch_daily_electric_usage(pool, site_id, date),
        fetch_circuit_anomalies(pool, site_id, date),
    )?;

    let pool_status = first_pool_status(pool, &view).await;
    let livestock_summary = first_livestock_summary(pool, &view, date).await;
    let septic_alert = site_septic_alert(pool, site_id).await;
    let active_experiments = resolve_active_experiments(pool, site_id, &view).await;
    let maintenance_due =
        gather_maintenance_due(pool, site_id, date, &scheduled_maintenance_events).await;
    let circuit_anomalies = merge_circuit_anomalies(circuit_anomalies, &anomaly_events);

    let baseline_comparison = match (&weather, total_kwh) {
        (Some(w), Some(actual)) => {
            compute_baseline_comparison(pool, site_id, w, actual).await.ok()
        }
        _ => None,
    };
    let estimated_cost = total_kwh.map(|kwh| kwh * 0.11);

    Ok(BriefingContext {
        date,
        site_id,
        weather,
        total_kwh,
        estimated_cost,
        baseline_comparison,
        circuit_anomalies,
        maintenance_due,
        active_experiments,
        pool_status,
        livestock_summary,
        septic_alert,
    })
}

// ---------------------------------------------------------------------------
// Time helpers
// ---------------------------------------------------------------------------

fn day_start_utc(d: NaiveDate) -> DateTime<Utc> {
    Utc.from_utc_datetime(&d.and_hms_opt(0, 0, 0).expect("valid midnight"))
}

fn day_end_utc(d: NaiveDate) -> DateTime<Utc> {
    day_start_utc(d + chrono::Duration::days(1))
}

// ---------------------------------------------------------------------------
// Neighbor projections
// ---------------------------------------------------------------------------

async fn first_pool_status(pool_conn: &PgPool, view: &query::ObjectView) -> Option<PoolDayStatus> {
    let obj = view
        .neighbors
        .iter()
        .find_map(|(_, o)| (o.kind == "pool").then_some(o))?;
    match lothal_db::water::get_pool(pool_conn, obj.id).await {
        Ok(Some(p)) => Some(PoolDayStatus {
            pool_name: p.name,
            // TODO: runtime hours when pool.pump_device_id is set.
            pump_runtime_hours: None,
        }),
        Ok(None) => None,
        Err(e) => {
            tracing::warn!("pool repo lookup failed: {e}");
            None
        }
    }
}

async fn first_livestock_summary(
    pool_conn: &PgPool,
    view: &query::ObjectView,
    date: NaiveDate,
) -> Option<LivestockDaySummary> {
    let obj = view
        .neighbors
        .iter()
        .find_map(|(_, o)| (o.kind == "flock").then_some(o))?;
    let flock = match lothal_db::livestock::get_flock(pool_conn, obj.id).await {
        Ok(Some(f)) => f,
        Ok(None) => return None,
        Err(e) => {
            tracing::warn!("flock repo lookup failed: {e}");
            return None;
        }
    };
    match lothal_db::livestock::get_flock_daily_summary(pool_conn, flock.id, date).await {
        Ok(s) => Some(LivestockDaySummary {
            flock_name: flock.name,
            eggs: s.eggs,
            feed_lbs: s.feed_lbs,
            mortality: s.mortality,
        }),
        Err(e) => {
            tracing::warn!("flock daily summary failed: {e}");
            None
        }
    }
}

async fn site_septic_alert(pool_conn: &PgPool, site_id: Uuid) -> Option<SepticAlert> {
    // SepticSystem has no `Describe` impl today, so it isn't reachable from
    // the site's neighbor list. Fetch directly by site_id.
    let septic = match lothal_db::water::get_septic_system(pool_conn, site_id).await {
        Ok(Some(s)) => s,
        Ok(None) => return None,
        Err(e) => {
            tracing::warn!("septic repo lookup failed: {e}");
            return None;
        }
    };
    let days = septic.days_until_pump()?;
    if days > 90 {
        return None;
    }
    Some(SepticAlert {
        days_until_pump: days,
        is_overdue: days < 0,
    })
}

async fn resolve_active_experiments(
    pool_conn: &PgPool,
    site_id: Uuid,
    view: &query::ObjectView,
) -> Vec<ActiveExperiment> {
    let experiment_ids: Vec<Uuid> = view
        .neighbors
        .iter()
        .filter_map(|(_, o)| (o.kind == "experiment").then_some(o.id))
        .collect();
    if experiment_ids.is_empty() {
        return Vec::new();
    }
    let experiments = match lothal_db::experiment::list_experiments_by_site(pool_conn, site_id).await
    {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("list_experiments_by_site failed: {e}");
            return Vec::new();
        }
    };
    let hypotheses = match lothal_db::experiment::list_hypotheses_by_site(pool_conn, site_id).await
    {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("list_hypotheses_by_site failed: {e}");
            return Vec::new();
        }
    };
    let hypothesis_by_id: std::collections::HashMap<_, _> =
        hypotheses.into_iter().map(|h| (h.id, h)).collect();

    experiments
        .into_iter()
        .filter(|e| experiment_ids.contains(&e.id))
        .filter(|e| {
            matches!(
                e.status,
                lothal_core::ontology::experiment::ExperimentStatus::Active
            )
        })
        .filter_map(|e| {
            hypothesis_by_id
                .get(&e.hypothesis_id)
                .map(|h| ActiveExperiment {
                    title: h.title.clone(),
                })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Maintenance & anomaly merge: ontology events + repo fallback
// ---------------------------------------------------------------------------

async fn gather_maintenance_due(
    pool_conn: &PgPool,
    site_id: Uuid,
    today: NaiveDate,
    scheduled_events: &[EventRecord],
) -> Vec<MaintenanceDue> {
    let within = today + chrono::Duration::days(7);
    let mut due: Vec<MaintenanceDue> = Vec::new();

    for ev in scheduled_events {
        let due_date = ev
            .properties
            .0
            .get("due_date")
            .and_then(|v| v.as_str())
            .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
            .unwrap_or_else(|| ev.time.date_naive());
        if due_date < today || due_date > within {
            continue;
        }
        due.push(MaintenanceDue {
            description: ev.summary.clone(),
            due_date,
        });
    }

    // Fallback to the repo-owned `maintenance_events.next_due` path for
    // events that predate the ontology `maintenance_scheduled` stream.
    match lothal_db::maintenance::get_upcoming_maintenance(pool_conn, site_id).await {
        Ok(rows) => {
            for m in rows {
                let Some(next_due) = m.next_due else { continue };
                if next_due < today || next_due > within {
                    continue;
                }
                if due
                    .iter()
                    .any(|d| d.due_date == next_due && d.description == m.description)
                {
                    continue;
                }
                due.push(MaintenanceDue {
                    description: m.description,
                    due_date: next_due,
                });
            }
        }
        Err(e) => tracing::warn!("get_upcoming_maintenance failed: {e}"),
    }

    due.sort_by_key(|d| d.due_date);
    due
}

fn merge_circuit_anomalies(
    mut from_readings: Vec<CircuitAnomaly>,
    anomaly_events: &[EventRecord],
) -> Vec<CircuitAnomaly> {
    // Only surface circuit-shaped anomaly events — site-wide deviations live
    // on `baseline_comparison`.
    for ev in anomaly_events {
        let is_circuit = ev
            .properties
            .0
            .get("source_type")
            .and_then(|v| v.as_str())
            == Some("circuit");
        if !is_circuit {
            continue;
        }
        let label = ev
            .properties
            .0
            .get("label")
            .and_then(|v| v.as_str())
            .unwrap_or(&ev.summary)
            .to_string();
        if from_readings.iter().any(|a| a.circuit_label == label) {
            continue;
        }
        let actual = ev
            .properties
            .0
            .get("value")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        let avg = ev
            .properties
            .0
            .get("baseline_value")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        from_readings.push(CircuitAnomaly {
            circuit_label: label,
            actual_hours: actual,
            avg_hours: avg,
        });
    }
    from_readings
}

// ---------------------------------------------------------------------------
// Non-ontology data paths: weather, readings, baseline
// ---------------------------------------------------------------------------

async fn fetch_weather_summary(
    pool: &PgPool,
    site_id: Uuid,
    date: NaiveDate,
) -> Result<Option<WeatherSummary>, sqlx::Error> {
    let row = sqlx::query_as::<_, (f64, f64, f64)>(
        r#"SELECT
               AVG(temperature_f) as avg_temp,
               MIN(temperature_f) as min_temp,
               MAX(temperature_f) as max_temp
           FROM weather_observations
           WHERE site_id = $1
             AND time >= $2::date
             AND time < ($2::date + interval '1 day')"#,
    )
    .bind(site_id)
    .bind(date)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|(avg, min, max)| {
        let base = 65.0;
        WeatherSummary {
            avg_temp_f: avg,
            min_temp_f: min,
            max_temp_f: max,
            cooling_degree_days: (avg - base).max(0.0),
            heating_degree_days: (base - avg).max(0.0),
        }
    }))
}

async fn fetch_daily_electric_usage(
    pool: &PgPool,
    site_id: Uuid,
    date: NaiveDate,
) -> Result<Option<f64>, sqlx::Error> {
    let row = sqlx::query_as::<_, (Option<f64>,)>(
        r#"SELECT SUM(rd.sum_value)
           FROM readings_daily rd
           JOIN circuits c ON rd.source_id = c.id AND rd.source_type = 'circuit'
           JOIN panels p ON c.panel_id = p.id
           JOIN structures s ON p.structure_id = s.id
           WHERE s.site_id = $1
             AND rd.bucket = $2::date
             AND rd.kind = 'electric_kwh'"#,
    )
    .bind(site_id)
    .bind(date)
    .fetch_optional(pool)
    .await?;

    Ok(row.and_then(|(v,)| v))
}

async fn fetch_circuit_anomalies(
    pool: &PgPool,
    site_id: Uuid,
    date: NaiveDate,
) -> Result<Vec<CircuitAnomaly>, sqlx::Error> {
    // Readings-derived circuit-runtime anomalies (mirrors
    // `lothal_ai::anomaly::detect_circuit_anomalies`). When anomaly sweep
    // events land in the ontology, `merge_circuit_anomalies` dedupes them.
    let rows = sqlx::query_as::<_, (String, f64, f64)>(
        r#"WITH yesterday AS (
               SELECT source_id, SUM(sum_value) as total
               FROM readings_daily
               WHERE bucket = $2::date
                 AND kind = 'electric_kwh'
                 AND source_type = 'circuit'
               GROUP BY source_id
           ),
           avg_14d AS (
               SELECT source_id, AVG(sum_value) as avg_total
               FROM readings_daily
               WHERE bucket >= ($2::date - interval '14 days')
                 AND bucket < $2::date
                 AND kind = 'electric_kwh'
                 AND source_type = 'circuit'
               GROUP BY source_id
               HAVING COUNT(*) >= 7
           )
           SELECT c.label, y.total, a.avg_total
           FROM yesterday y
           JOIN avg_14d a ON y.source_id = a.source_id
           JOIN circuits c ON y.source_id = c.id
           JOIN panels p ON c.panel_id = p.id
           JOIN structures s ON p.structure_id = s.id
           WHERE s.site_id = $1
             AND a.avg_total > 0
             AND y.total > a.avg_total * 1.5"#,
    )
    .bind(site_id)
    .bind(date)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|(label, actual, avg)| CircuitAnomaly {
            circuit_label: label,
            actual_hours: actual,
            avg_hours: avg,
        })
        .collect())
}

async fn compute_baseline_comparison(
    pool: &PgPool,
    site_id: Uuid,
    weather: &WeatherSummary,
    actual_kwh: f64,
) -> Result<BaselineComparison, AiError> {
    let accounts = lothal_db::bill::list_utility_accounts_by_site(pool, site_id).await?;

    let electric_account = accounts
        .iter()
        .find(|a| a.utility_type.to_string().to_lowercase() == "electric")
        .ok_or_else(|| AiError::Validation("No electric account found".into()))?;

    let bills = lothal_db::bill::list_bills_by_account(pool, electric_account.id).await?;

    if bills.len() < 3 {
        return Err(AiError::Validation("Not enough bills for baseline".into()));
    }

    let weather_days = lothal_db::weather::get_daily_weather_summaries(
        pool,
        site_id,
        bills.first().unwrap().period.range.start,
        bills.last().unwrap().period.range.end,
    )
    .await?;

    let weather_map: std::collections::HashMap<NaiveDate, _> =
        weather_days.iter().map(|w| (w.date, w)).collect();

    let base_temp = 65.0;
    let mut data_points = Vec::new();

    for bill in &bills {
        let daily_usage = match bill.daily_usage() {
            Some(u) => u,
            None => continue,
        };
        for date in bill.period.range.iter_days() {
            if let Some(w) = weather_map.get(&date) {
                data_points.push(lothal_engine::baseline::DailyDataPoint {
                    date,
                    usage: daily_usage,
                    cooling_degree_days: (w.avg_temp_f - base_temp).max(0.0),
                    heating_degree_days: (base_temp - w.avg_temp_f).max(0.0),
                });
            }
        }
    }

    if data_points.len() < 3 {
        return Err(AiError::Validation("Insufficient data for baseline".into()));
    }

    let mode = if weather.cooling_degree_days > weather.heating_degree_days {
        lothal_engine::baseline::BaselineMode::Cooling
    } else {
        lothal_engine::baseline::BaselineMode::Heating
    };

    let model = lothal_engine::baseline::compute_baseline(&data_points, mode)
        .map_err(|e| AiError::Validation(format!("Baseline computation failed: {e}")))?;

    let degree_days = match mode {
        lothal_engine::baseline::BaselineMode::Cooling => weather.cooling_degree_days,
        lothal_engine::baseline::BaselineMode::Heating => weather.heating_degree_days,
    };

    let predicted = lothal_engine::baseline::predict_usage(&model, degree_days);
    let deviation_pct = if predicted > 0.0 {
        ((actual_kwh - predicted) / predicted) * 100.0
    } else {
        0.0
    };

    Ok(BaselineComparison {
        predicted_kwh: predicted,
        actual_kwh,
        deviation_pct,
    })
}
