use chrono::{DateTime, Duration, Timelike, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::AiError;

/// A detected power event on a circuit — a period of elevated power consumption.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PowerSignature {
    pub circuit_id: Uuid,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub duration_minutes: f64,
    pub peak_watts: f64,
    pub avg_watts: f64,
    pub total_kwh: f64,
    pub pattern: PowerPattern,
    pub time_of_day: String,
    pub day_of_week: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PowerPattern {
    /// Steady load (< 20% variation).
    Steady,
    /// Cycling on/off (e.g., compressor, fridge).
    Cycling,
    /// Gradual ramp up then steady.
    Ramp,
    /// Short high-power burst.
    Burst,
    /// Other / unclassified.
    Variable,
}

/// Minimum watts above idle to consider a "power event".
const EVENT_THRESHOLD_WATTS: f64 = 50.0;
/// Minimum event duration to be worth classifying.
const MIN_DURATION_MINUTES: f64 = 2.0;
/// Coefficient of variation threshold for "steady" classification.
const STEADY_CV_THRESHOLD: f64 = 0.20;

/// Extract power signatures from readings for a circuit over a time window.
pub async fn extract_signatures(
    pool: &PgPool,
    circuit_id: Uuid,
    window_days: u32,
) -> Result<Vec<PowerSignature>, AiError> {
    let end = Utc::now();
    let start = end - Duration::days(i64::from(window_days));

    // Fetch watt readings, ordered by time.
    let rows = sqlx::query_as::<_, (DateTime<Utc>, f64)>(
        r#"SELECT time, value
           FROM readings
           WHERE source_type = 'circuit'
             AND source_id = $1
             AND kind = 'electric_watts'
             AND time >= $2
             AND time <= $3
           ORDER BY time"#,
    )
    .bind(circuit_id)
    .bind(start)
    .bind(end)
    .fetch_all(pool)
    .await?;

    if rows.is_empty() {
        return Ok(Vec::new());
    }

    // Compute a rough idle baseline (10th percentile of readings).
    let mut values: Vec<f64> = rows.iter().map(|(_, v)| *v).collect();
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let idx_10 = (values.len() as f64 * 0.10) as usize;
    let idle_watts = values.get(idx_10).copied().unwrap_or(0.0);

    let threshold = idle_watts + EVENT_THRESHOLD_WATTS;

    // Segment into events: contiguous runs above threshold.
    let mut signatures = Vec::new();
    let mut event_start: Option<usize> = None;

    for (i, (_, watts)) in rows.iter().enumerate() {
        if *watts >= threshold {
            if event_start.is_none() {
                event_start = Some(i);
            }
        } else if let Some(start_idx) = event_start.take() {
            if let Some(sig) = build_signature(circuit_id, &rows[start_idx..i]) {
                signatures.push(sig);
            }
        }
    }

    // Handle event that extends to end of data.
    if let Some(start_idx) = event_start {
        if let Some(sig) = build_signature(circuit_id, &rows[start_idx..]) {
            signatures.push(sig);
        }
    }

    // Deduplicate similar signatures (keep representative samples).
    // For now, limit to at most 50 signatures for LLM context size.
    if signatures.len() > 50 {
        // Keep a representative sample: sort by peak_watts, take every Nth.
        signatures.sort_by(|a, b| {
            b.peak_watts
                .partial_cmp(&a.peak_watts)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let step = signatures.len() / 50;
        signatures = signatures.into_iter().step_by(step.max(1)).take(50).collect();
    }

    Ok(signatures)
}

fn build_signature(
    circuit_id: Uuid,
    readings: &[(DateTime<Utc>, f64)],
) -> Option<PowerSignature> {
    if readings.len() < 2 {
        return None;
    }

    let start_time = readings.first().unwrap().0;
    let end_time = readings.last().unwrap().0;
    let duration_minutes = (end_time - start_time).num_seconds() as f64 / 60.0;

    if duration_minutes < MIN_DURATION_MINUTES {
        return None;
    }

    let watts: Vec<f64> = readings.iter().map(|(_, w)| *w).collect();
    let peak_watts = watts.iter().cloned().fold(0.0_f64, f64::max);
    let avg_watts = watts.iter().sum::<f64>() / watts.len() as f64;
    let total_kwh = avg_watts * duration_minutes / 60.0 / 1000.0;

    let pattern = classify_pattern(&watts);
    let time_of_day = classify_time_of_day(start_time);
    let day_of_week = start_time.format("%A").to_string();

    Some(PowerSignature {
        circuit_id,
        start_time,
        end_time,
        duration_minutes,
        peak_watts,
        avg_watts,
        total_kwh,
        pattern,
        time_of_day,
        day_of_week,
    })
}

fn classify_pattern(watts: &[f64]) -> PowerPattern {
    if watts.len() < 3 {
        return PowerPattern::Burst;
    }

    let mean = watts.iter().sum::<f64>() / watts.len() as f64;
    let variance = watts.iter().map(|w| (w - mean).powi(2)).sum::<f64>() / watts.len() as f64;
    let std_dev = variance.sqrt();
    let cv = if mean > 0.0 { std_dev / mean } else { 0.0 };

    // Check for cycling: count zero crossings relative to mean.
    let crossings = watts
        .windows(2)
        .filter(|w| (w[0] - mean).signum() != (w[1] - mean).signum())
        .count();
    let crossing_rate = crossings as f64 / watts.len() as f64;

    // Check for ramp: monotonic increase in first third.
    let third = watts.len() / 3;
    let is_ramp = third > 2
        && watts[..third]
            .windows(2)
            .filter(|w| w[1] >= w[0])
            .count()
            > third * 2 / 3;

    if watts.len() <= 5 {
        PowerPattern::Burst
    } else if cv < STEADY_CV_THRESHOLD {
        PowerPattern::Steady
    } else if crossing_rate > 0.15 {
        PowerPattern::Cycling
    } else if is_ramp {
        PowerPattern::Ramp
    } else {
        PowerPattern::Variable
    }
}

fn classify_time_of_day(time: DateTime<Utc>) -> String {
    // Convert to approximate local time (Oklahoma is UTC-6 or UTC-5).
    let hour = (time.hour() as i32 - 6).rem_euclid(24);
    match hour {
        5..=11 => "morning".to_string(),
        12..=16 => "afternoon".to_string(),
        17..=20 => "evening".to_string(),
        _ => "night".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_steady() {
        let watts = vec![100.0, 102.0, 98.0, 101.0, 99.0, 100.0, 103.0, 97.0];
        assert!(matches!(classify_pattern(&watts), PowerPattern::Steady));
    }

    #[test]
    fn test_classify_cycling() {
        let watts: Vec<f64> = (0..40)
            .map(|i| if i % 4 < 2 { 2000.0 } else { 200.0 })
            .collect();
        assert!(matches!(classify_pattern(&watts), PowerPattern::Cycling));
    }

    #[test]
    fn test_classify_burst() {
        let watts = vec![3000.0, 3200.0, 2800.0];
        assert!(matches!(classify_pattern(&watts), PowerPattern::Burst));
    }

    #[test]
    fn test_time_of_day() {
        // 18:00 UTC = 12:00 CT (afternoon)
        let t = chrono::DateTime::parse_from_rfc3339("2026-04-12T18:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(classify_time_of_day(t), "afternoon");
    }
}
