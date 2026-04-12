//! Anomaly detection: surface circuits or whole-site usage that deviate from
//! their weather-normalized baseline.
//!
//! Runs from the scheduler daemon every 15 minutes. Results are persisted to
//! `anomaly_alerts` and delivered via `briefing::format::BriefingOutput`.

use chrono::{NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::AiError;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnomalyKind {
    /// Circuit's daily kWh exceeded its 14-day average by >50%.
    CircuitRuntime,
    /// Site-wide daily kWh deviated from the weather-normalized baseline by
    /// more than the configured threshold.
    SiteBaselineDeviation,
}

impl AnomalyKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::CircuitRuntime => "circuit_runtime",
            Self::SiteBaselineDeviation => "site_baseline_deviation",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Anomaly {
    pub site_id: Uuid,
    pub source_type: String,
    pub source_id: Uuid,
    pub kind: AnomalyKind,
    pub value: f64,
    pub baseline_value: f64,
    pub deviation_pct: f64,
    pub message: String,
}

// ---------------------------------------------------------------------------
// Sweep
// ---------------------------------------------------------------------------

/// Run all anomaly detectors for a site on a given date.
///
/// Does not dedupe or persist. Callers that want dedupe-against-prior-alerts
/// should run results through [`filter_duplicates`] before persisting.
pub async fn sweep(
    pool: &PgPool,
    site_id: Uuid,
    date: NaiveDate,
) -> Result<Vec<Anomaly>, AiError> {
    let (circuit, site) = tokio::join!(
        detect_circuit_anomalies(pool, site_id, date),
        detect_site_baseline_deviation(pool, site_id, date),
    );

    let mut out = Vec::new();
    match circuit {
        Ok(mut v) => out.append(&mut v),
        Err(e) => tracing::warn!("circuit anomaly detection failed: {e}"),
    }
    match site {
        Ok(Some(a)) => out.push(a),
        Ok(None) => {}
        Err(e) => tracing::warn!("site baseline deviation detection failed: {e}"),
    }

    Ok(out)
}

// ---------------------------------------------------------------------------
// Detectors
// ---------------------------------------------------------------------------

async fn detect_circuit_anomalies(
    pool: &PgPool,
    site_id: Uuid,
    date: NaiveDate,
) -> Result<Vec<Anomaly>, AiError> {
    let rows = sqlx::query_as::<_, (Uuid, String, f64, f64)>(
        r#"WITH yesterday AS (
               SELECT source_id, SUM(sum_value) AS total
               FROM readings_daily
               WHERE bucket = $2::date
                 AND kind = 'electric_kwh'
                 AND source_type = 'circuit'
               GROUP BY source_id
           ),
           avg_14d AS (
               SELECT source_id, AVG(sum_value) AS avg_total
               FROM readings_daily
               WHERE bucket >= ($2::date - interval '14 days')
                 AND bucket < $2::date
                 AND kind = 'electric_kwh'
                 AND source_type = 'circuit'
               GROUP BY source_id
               HAVING COUNT(*) >= 7
           )
           SELECT c.id, c.label, y.total, a.avg_total
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
        .map(|(circuit_id, label, actual, avg)| {
            let deviation_pct = ((actual - avg) / avg) * 100.0;
            let message = format!(
                "Circuit '{label}' used {actual:.1} kWh vs {avg:.1} kWh 14-day average ({deviation_pct:+.0}%)"
            );
            Anomaly {
                site_id,
                source_type: "circuit".to_string(),
                source_id: circuit_id,
                kind: AnomalyKind::CircuitRuntime,
                value: actual,
                baseline_value: avg,
                deviation_pct,
                message,
            }
        })
        .collect())
}

/// Compare yesterday's site-wide kWh against the weather-normalized baseline.
///
/// Threshold: alert when actual deviates from predicted by more than 15% in
/// either direction *and* actual is at least 5 kWh higher (absolute floor
/// suppresses noise on tiny-usage days).
async fn detect_site_baseline_deviation(
    pool: &PgPool,
    site_id: Uuid,
    date: NaiveDate,
) -> Result<Option<Anomaly>, AiError> {
    let total_kwh = fetch_site_total_kwh(pool, site_id, date).await?;
    let actual = match total_kwh {
        Some(v) if v > 0.0 => v,
        _ => return Ok(None),
    };

    let avg_temp = match fetch_avg_temp_f(pool, site_id, date).await? {
        Some(v) => v,
        None => return Ok(None),
    };

    let baseline = match compute_site_baseline(pool, site_id, avg_temp).await {
        Ok(b) => b,
        Err(AiError::Validation(_)) => return Ok(None),
        Err(e) => return Err(e),
    };

    let deviation_pct = ((actual - baseline.predicted_kwh) / baseline.predicted_kwh) * 100.0;

    if deviation_pct.abs() < 15.0 || (actual - baseline.predicted_kwh).abs() < 5.0 {
        return Ok(None);
    }

    let direction = if deviation_pct > 0.0 { "above" } else { "below" };
    let message = format!(
        "Site used {actual:.1} kWh vs {:.1} kWh predicted ({deviation_pct:+.0}%, {direction} weather-normalized baseline)",
        baseline.predicted_kwh
    );

    Ok(Some(Anomaly {
        site_id,
        source_type: "site".to_string(),
        source_id: site_id,
        kind: AnomalyKind::SiteBaselineDeviation,
        value: actual,
        baseline_value: baseline.predicted_kwh,
        deviation_pct,
        message,
    }))
}

// ---------------------------------------------------------------------------
// Baseline helper (shared with briefing context — could be extracted later)
// ---------------------------------------------------------------------------

struct BaselinePrediction {
    predicted_kwh: f64,
}

async fn compute_site_baseline(
    pool: &PgPool,
    site_id: Uuid,
    avg_temp_f: f64,
) -> Result<BaselinePrediction, AiError> {
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

    let weather_map: std::collections::HashMap<NaiveDate, &lothal_db::weather::DailyWeatherRow> =
        weather_days.iter().map(|w| (w.date, w)).collect();

    let base_temp = 65.0;
    let mut data_points = Vec::new();
    for bill in &bills {
        let daily_usage = match bill.daily_usage() {
            Some(u) => u,
            None => continue,
        };
        for d in bill.period.range.iter_days() {
            if let Some(w) = weather_map.get(&d) {
                data_points.push(lothal_engine::baseline::DailyDataPoint {
                    date: d,
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

    let cdd = (avg_temp_f - base_temp).max(0.0);
    let hdd = (base_temp - avg_temp_f).max(0.0);
    let mode = if cdd > hdd {
        lothal_engine::baseline::BaselineMode::Cooling
    } else {
        lothal_engine::baseline::BaselineMode::Heating
    };

    let model = lothal_engine::baseline::compute_baseline(&data_points, mode)
        .map_err(|e| AiError::Validation(format!("Baseline computation failed: {e}")))?;

    let degree_days = if cdd > hdd { cdd } else { hdd };
    let predicted = lothal_engine::baseline::predict_usage(&model, degree_days);

    Ok(BaselinePrediction {
        predicted_kwh: predicted,
    })
}

async fn fetch_site_total_kwh(
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

async fn fetch_avg_temp_f(
    pool: &PgPool,
    site_id: Uuid,
    date: NaiveDate,
) -> Result<Option<f64>, sqlx::Error> {
    let row = sqlx::query_as::<_, (Option<f64>,)>(
        r#"SELECT AVG(temperature_f)
           FROM weather_observations
           WHERE site_id = $1
             AND time >= $2::date
             AND time < ($2::date + interval '1 day')"#,
    )
    .bind(site_id)
    .bind(date)
    .fetch_optional(pool)
    .await?;

    Ok(row.and_then(|(v,)| v))
}

// ---------------------------------------------------------------------------
// Dedupe + persistence
// ---------------------------------------------------------------------------

/// Drop anomalies that have already been alerted for the same source+kind in
/// the last 24h unless the deviation has grown by more than 20 percentage
/// points since the last alert (escalation).
pub async fn filter_duplicates(
    pool: &PgPool,
    candidates: Vec<Anomaly>,
) -> Result<Vec<Anomaly>, AiError> {
    let mut kept = Vec::with_capacity(candidates.len());
    for a in candidates {
        let prior = sqlx::query_as::<_, (f64,)>(
            r#"SELECT deviation_pct
               FROM anomaly_alerts
               WHERE source_id = $1
                 AND kind = $2
                 AND detected_at > now() - interval '24 hours'
               ORDER BY detected_at DESC
               LIMIT 1"#,
        )
        .bind(a.source_id)
        .bind(a.kind.as_str())
        .fetch_optional(pool)
        .await?;

        let should_alert = match prior {
            None => true,
            Some((prev_dev,)) => (a.deviation_pct.abs() - prev_dev.abs()) > 20.0,
        };

        if should_alert {
            kept.push(a);
        }
    }
    Ok(kept)
}

/// Insert detected anomalies into `anomaly_alerts`.
pub async fn persist(pool: &PgPool, anomalies: &[Anomaly]) -> Result<(), AiError> {
    for a in anomalies {
        sqlx::query(
            r#"INSERT INTO anomaly_alerts
                   (site_id, source_type, source_id, kind, detected_at,
                    value, baseline_value, deviation_pct, message, status)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, 'detected')"#,
        )
        .bind(a.site_id)
        .bind(&a.source_type)
        .bind(a.source_id)
        .bind(a.kind.as_str())
        .bind(Utc::now())
        .bind(a.value)
        .bind(a.baseline_value)
        .bind(a.deviation_pct)
        .bind(&a.message)
        .execute(pool)
        .await?;
    }
    Ok(())
}

/// Mark an alert as delivered (push sent successfully).
pub async fn mark_delivered(pool: &PgPool, alert_ids: &[Uuid]) -> Result<(), AiError> {
    if alert_ids.is_empty() {
        return Ok(());
    }
    sqlx::query(
        "UPDATE anomaly_alerts SET delivered_at = now(), status = 'delivered' WHERE id = ANY($1)",
    )
    .bind(alert_ids)
    .execute(pool)
    .await?;
    Ok(())
}
