use chrono::NaiveDate;
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

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

/// Gather all context data for a daily briefing from the database.
pub async fn gather_context(
    pool: &PgPool,
    site_id: Uuid,
    date: NaiveDate,
) -> Result<BriefingContext, AiError> {
    let (weather, total_kwh, circuit_anomalies, maintenance_due, active_experiments) = tokio::try_join!(
        fetch_weather_summary(pool, site_id, date),
        fetch_daily_electric_usage(pool, site_id, date),
        fetch_circuit_anomalies(pool, site_id, date),
        fetch_maintenance_due(pool, site_id, date),
        fetch_active_experiments(pool, site_id),
    )?;

    // Property operations context — fetched in parallel. Errors are logged but
    // do not block the briefing (a missing query result and a failed query
    // both degrade gracefully, but only one is a bug).
    let (pool_status, livestock_summary, septic_alert) = tokio::join!(
        fetch_pool_status(pool, site_id),
        fetch_livestock_summary(pool, site_id, date),
        fetch_septic_alert(pool, site_id),
    );
    let pool_status = match pool_status {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("pool status query failed: {e}");
            None
        }
    };
    let livestock_summary = match livestock_summary {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("livestock summary query failed: {e}");
            None
        }
    };
    let septic_alert = match septic_alert {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("septic alert query failed: {e}");
            None
        }
    };

    // Compute baseline comparison if we have weather and usage data.
    let baseline_comparison = match (&weather, total_kwh) {
        (Some(w), Some(actual)) => compute_baseline_comparison(pool, site_id, w, actual).await.ok(),
        _ => None,
    };

    let estimated_cost = total_kwh.map(|kwh| {
        // Default to Oklahoma average residential rate if no rate schedule found.
        kwh * 0.11
    });

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
    // Sum kWh readings from the daily continuous aggregate for electric sources.
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
    // Find circuits where yesterday's runtime was >50% above the 14-day average.
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

async fn fetch_maintenance_due(
    pool: &PgPool,
    site_id: Uuid,
    date: NaiveDate,
) -> Result<Vec<MaintenanceDue>, sqlx::Error> {
    let within = date + chrono::Duration::days(7);

    let rows = sqlx::query_as::<_, (String, NaiveDate)>(
        r#"SELECT
               COALESCE(d.label, s.address, me.maintenance_type::text) as description,
               me.next_due
           FROM maintenance_events me
           LEFT JOIN devices d ON me.target_type = 'device' AND me.target_id = d.id
           LEFT JOIN structures s ON me.target_type = 'structure' AND me.target_id = s.id
           WHERE (
               (me.target_type = 'device' AND me.target_id IN (
                   SELECT d2.id FROM devices d2
                   JOIN structures s2 ON d2.structure_id = s2.id
                   WHERE s2.site_id = $1
               ))
               OR (me.target_type = 'structure' AND me.target_id IN (
                   SELECT s3.id FROM structures s3 WHERE s3.site_id = $1
               ))
           )
           AND me.next_due IS NOT NULL
           AND me.next_due <= $3
           AND me.next_due >= $2
           ORDER BY me.next_due"#,
    )
    .bind(site_id)
    .bind(date)
    .bind(within)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|(desc, due)| MaintenanceDue {
            description: desc,
            due_date: due,
        })
        .collect())
}

async fn fetch_active_experiments(
    pool: &PgPool,
    site_id: Uuid,
) -> Result<Vec<ActiveExperiment>, sqlx::Error> {
    let rows = sqlx::query_as::<_, (String,)>(
        r#"SELECT h.title
           FROM experiments e
           JOIN hypotheses h ON e.hypothesis_id = h.id
           WHERE h.site_id = $1 AND e.status = 'active'"#,
    )
    .bind(site_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|(title,)| ActiveExperiment { title })
        .collect())
}

async fn compute_baseline_comparison(
    pool: &PgPool,
    site_id: Uuid,
    weather: &WeatherSummary,
    actual_kwh: f64,
) -> Result<BaselineComparison, AiError> {
    // Fetch the most recent bills to compute a baseline.
    let accounts = lothal_db::bill::list_utility_accounts_by_site(pool, site_id).await?;

    let electric_account = accounts
        .iter()
        .find(|a| a.utility_type.to_string().to_lowercase() == "electric")
        .ok_or_else(|| AiError::Validation("No electric account found".into()))?;

    let bills =
        lothal_db::bill::list_bills_by_account(pool, electric_account.id).await?;

    if bills.len() < 3 {
        return Err(AiError::Validation("Not enough bills for baseline".into()));
    }

    // Build daily data points from bills + weather for baseline computation.
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

    // Use cooling or heating baseline depending on today's weather.
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

// ---------------------------------------------------------------------------
// Property operations context fetchers
// ---------------------------------------------------------------------------

async fn fetch_pool_status(
    pool: &PgPool,
    site_id: Uuid,
) -> Result<Option<PoolDayStatus>, sqlx::Error> {
    let pools = lothal_db::water::list_pools_by_site(pool, site_id).await?;
    let first = match pools.into_iter().next() {
        Some(p) => p,
        None => return Ok(None),
    };

    Ok(Some(PoolDayStatus {
        pool_name: first.name,
        pump_runtime_hours: None, // TODO: query from readings when pump_device_id is set
    }))
}

async fn fetch_livestock_summary(
    pool: &PgPool,
    site_id: Uuid,
    date: NaiveDate,
) -> Result<Option<LivestockDaySummary>, sqlx::Error> {
    let flocks = lothal_db::livestock::list_flocks_by_site(pool, site_id).await?;
    let flock = match flocks.into_iter().next() {
        Some(f) => f,
        None => return Ok(None),
    };

    let summary = lothal_db::livestock::get_flock_daily_summary(pool, flock.id, date).await?;
    Ok(Some(LivestockDaySummary {
        flock_name: flock.name,
        eggs: summary.eggs,
        feed_lbs: summary.feed_lbs,
        mortality: summary.mortality,
    }))
}

async fn fetch_septic_alert(
    pool: &PgPool,
    site_id: Uuid,
) -> Result<Option<SepticAlert>, sqlx::Error> {
    let septic = match lothal_db::water::get_septic_system(pool, site_id).await? {
        Some(s) => s,
        None => return Ok(None),
    };

    let days = match septic.days_until_pump() {
        Some(d) => d,
        None => return Ok(None),
    };

    // Only alert if within 90 days or overdue
    if days > 90 {
        return Ok(None);
    }

    Ok(Some(SepticAlert {
        days_until_pump: days,
        is_overdue: days < 0,
    }))
}
