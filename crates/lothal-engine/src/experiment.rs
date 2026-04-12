//! Experiment evaluation — compare baseline and result periods to quantify
//! the impact of an intervention, optionally normalizing for weather.

use serde::{Deserialize, Serialize};

use lothal_core::Usd;

use crate::baseline::{BaselineModel, DailyDataPoint};
use crate::EngineError;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// The quantified outcome of an experiment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperimentEvaluation {
    /// Identifier carried through from the caller (e.g. `Experiment::id`).
    pub experiment_id: String,
    pub baseline_avg_daily_usage: f64,
    pub result_avg_daily_usage: f64,
    /// Simple (raw) percentage change: (baseline - result) / baseline.
    pub raw_change_pct: f64,
    /// Weather-normalized percentage change, if a baseline model was provided.
    pub weather_normalized_change_pct: Option<f64>,
    pub estimated_annual_savings_usd: Usd,
    /// 0.0 – 1.0 confidence in the evaluation.
    pub confidence_score: f64,
    /// Human-readable interpretation of the result.
    pub interpretation: String,
}

// ---------------------------------------------------------------------------
// Evaluation
// ---------------------------------------------------------------------------

/// Evaluate the impact of an intervention by comparing baseline-period data
/// to result-period data.
///
/// * `baseline_data` — daily data points from the period *before* the intervention.
/// * `result_data`   — daily data points from the period *after* the intervention.
/// * `baseline_model` — optional weather-regression model for normalization.
/// * `rate_per_kwh`   — $/kWh for converting savings to dollars.
pub fn evaluate_experiment(
    baseline_data: &[DailyDataPoint],
    result_data: &[DailyDataPoint],
    baseline_model: Option<&BaselineModel>,
    rate_per_kwh: f64,
) -> Result<ExperimentEvaluation, EngineError> {
    if baseline_data.is_empty() {
        return Err(EngineError::InsufficientData(
            "Baseline period has no data points".into(),
        ));
    }
    if result_data.is_empty() {
        return Err(EngineError::InsufficientData(
            "Result period has no data points".into(),
        ));
    }

    let baseline_avg = avg_usage(baseline_data);
    let result_avg = avg_usage(result_data);

    // Raw change: positive = savings (usage went down)
    let raw_change_pct = if baseline_avg.abs() > f64::EPSILON {
        (baseline_avg - result_avg) / baseline_avg
    } else {
        0.0
    };

    // Weather-normalized comparison
    let weather_normalized_change_pct = baseline_model.map(|model| {
        // Predict what usage *would have been* in the result period if no
        // intervention had occurred, using the result period's actual weather.
        let predicted_result_avg: f64 = result_data
            .iter()
            .map(|d| {
                let dd = d.cooling_degree_days.max(d.heating_degree_days);
                model.slope * dd + model.intercept
            })
            .sum::<f64>()
            / result_data.len() as f64;

        if predicted_result_avg.abs() > f64::EPSILON {
            (predicted_result_avg - result_avg) / predicted_result_avg
        } else {
            0.0
        }
    });

    let change_pct = weather_normalized_change_pct.unwrap_or(raw_change_pct);

    // Annual savings estimate
    let daily_savings_kwh = baseline_avg * change_pct;
    let annual_savings = daily_savings_kwh * 365.0 * rate_per_kwh;

    let r_squared = baseline_model.map(|m| m.r_squared);
    let confidence =
        compute_confidence(baseline_data.len(), result_data.len(), r_squared);

    let interpretation = build_interpretation(
        change_pct,
        weather_normalized_change_pct.is_some(),
        annual_savings,
        confidence,
    );

    Ok(ExperimentEvaluation {
        experiment_id: String::new(),
        baseline_avg_daily_usage: baseline_avg,
        result_avg_daily_usage: result_avg,
        raw_change_pct,
        weather_normalized_change_pct,
        estimated_annual_savings_usd: Usd::new(annual_savings),
        confidence_score: confidence,
        interpretation,
    })
}

// ---------------------------------------------------------------------------
// Confidence scoring
// ---------------------------------------------------------------------------

/// Compute a 0.0 – 1.0 confidence score based on data quality.
///
/// Factors:
/// * Data-point counts — minimum 14 days each for any meaningful result.
/// * R² of the baseline model — higher = better weather normalization.
pub fn compute_confidence(
    baseline_count: usize,
    result_count: usize,
    r_squared: Option<f64>,
) -> f64 {
    // --- count component ---
    // Ramp from 0 at 0 days to 1.0 at 30 days, with a hard floor at 14 days.
    let count_score = |n: usize| -> f64 {
        if n < 14 {
            // Below minimum — very low confidence
            (n as f64 / 14.0) * 0.3
        } else {
            // 14..30 linearly from 0.5 to 1.0, then cap at 1.0
            let t = ((n as f64 - 14.0) / 16.0).min(1.0);
            0.5 + 0.5 * t
        }
    };

    let baseline_score = count_score(baseline_count);
    let result_score = count_score(result_count);

    // --- R² component ---
    let r2_score = match r_squared {
        Some(r2) => {
            // R² < 0.3 is a poor model; R² > 0.8 is good.
            if r2 < 0.3 {
                0.3
            } else if r2 > 0.8 {
                1.0
            } else {
                // Linear interpolation from 0.3 to 1.0 over R² 0.3-0.8
                0.3 + (r2 - 0.3) / 0.5 * 0.7
            }
        }
        None => 0.5, // no model — medium weight
    };

    // Weighted combination
    let combined = 0.35 * baseline_score + 0.35 * result_score + 0.30 * r2_score;
    combined.clamp(0.0, 1.0)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn avg_usage(data: &[DailyDataPoint]) -> f64 {
    if data.is_empty() {
        return 0.0;
    }
    data.iter().map(|d| d.usage).sum::<f64>() / data.len() as f64
}

fn build_interpretation(
    change_pct: f64,
    is_weather_normalized: bool,
    annual_savings: f64,
    confidence: f64,
) -> String {
    let direction = if change_pct > 0.0 {
        "reduced"
    } else if change_pct < 0.0 {
        "increased"
    } else {
        "did not change"
    };

    let method = if is_weather_normalized {
        "weather-normalized"
    } else {
        "raw"
    };

    let confidence_label = if confidence >= 0.8 {
        "high"
    } else if confidence >= 0.5 {
        "moderate"
    } else {
        "low"
    };

    if change_pct.abs() < 0.001 {
        return format!(
            "Usage did not change meaningfully ({method}). Confidence: {confidence_label} ({confidence:.2})."
        );
    }

    format!(
        "Intervention {direction} usage by {:.1}% ({method}). \
         At current rates, this saves approximately ${:.2}/year. \
         Confidence: {confidence_label} ({confidence:.2}).",
        change_pct.abs() * 100.0,
        annual_savings.abs(),
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::baseline::{compute_baseline, BaselineMode, DailyDataPoint};
    use chrono::NaiveDate;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn make_data(start: NaiveDate, days: usize, daily_usage: f64, cdd: f64) -> Vec<DailyDataPoint> {
        (0..days)
            .map(|i| DailyDataPoint {
                date: start + chrono::Duration::days(i as i64),
                usage: daily_usage,
                cooling_degree_days: cdd,
                heating_degree_days: 0.0,
            })
            .collect()
    }

    #[test]
    fn test_clear_savings_no_model() {
        let baseline = make_data(d(2025, 6, 1), 30, 50.0, 15.0);
        let result = make_data(d(2025, 7, 1), 30, 40.0, 15.0);

        let eval = evaluate_experiment(&baseline, &result, None, 0.11).unwrap();

        assert!((eval.raw_change_pct - 0.20).abs() < 0.001, "expected ~20% raw savings");
        assert!(eval.weather_normalized_change_pct.is_none());
        assert!(eval.estimated_annual_savings_usd.value() > 0.0);
        assert!(eval.interpretation.contains("reduced"));
    }

    #[test]
    fn test_no_change() {
        let baseline = make_data(d(2025, 6, 1), 30, 50.0, 15.0);
        let result = make_data(d(2025, 7, 1), 30, 50.0, 15.0);

        let eval = evaluate_experiment(&baseline, &result, None, 0.11).unwrap();
        assert!(eval.raw_change_pct.abs() < 0.001);
    }

    #[test]
    fn test_usage_increased() {
        let baseline = make_data(d(2025, 6, 1), 30, 40.0, 15.0);
        let result = make_data(d(2025, 7, 1), 30, 50.0, 15.0);

        let eval = evaluate_experiment(&baseline, &result, None, 0.11).unwrap();
        assert!(eval.raw_change_pct < 0.0, "change should be negative (usage went up)");
        assert!(eval.interpretation.contains("increased"));
    }

    #[test]
    fn test_weather_normalized_evaluation() {
        // Baseline: usage = 2*CDD + 30, so at CDD=10 -> usage=50
        let baseline_pts: Vec<DailyDataPoint> = (0..30)
            .map(|i| {
                let cdd = 5.0 + (i as f64) * 0.5;
                DailyDataPoint {
                    date: d(2025, 6, 1) + chrono::Duration::days(i),
                    usage: 2.0 * cdd + 30.0,
                    cooling_degree_days: cdd,
                    heating_degree_days: 0.0,
                }
            })
            .collect();

        let model = compute_baseline(&baseline_pts, BaselineMode::Cooling).unwrap();
        assert!(model.r_squared > 0.99);

        // Result period: same weather but 10% less usage (intervention worked)
        let result_pts: Vec<DailyDataPoint> = (0..30)
            .map(|i| {
                let cdd = 5.0 + (i as f64) * 0.5;
                DailyDataPoint {
                    date: d(2025, 7, 1) + chrono::Duration::days(i),
                    usage: (2.0 * cdd + 30.0) * 0.90,
                    cooling_degree_days: cdd,
                    heating_degree_days: 0.0,
                }
            })
            .collect();

        let eval = evaluate_experiment(&baseline_pts, &result_pts, Some(&model), 0.11).unwrap();

        let wn = eval.weather_normalized_change_pct.unwrap();
        assert!(
            (wn - 0.10).abs() < 0.02,
            "weather-normalized change should be ~10%, got {:.2}%",
            wn * 100.0
        );
        assert!(eval.confidence_score > 0.7);
    }

    #[test]
    fn test_weather_normalized_hotter_result_period() {
        // Baseline at mild weather, result at hotter weather.
        // Without normalization the raw change looks bad; with normalization
        // it should show the intervention helped.

        // Baseline: usage = 2*CDD + 30 at CDD ~10
        let baseline_pts: Vec<DailyDataPoint> = (0..30)
            .map(|i| {
                let cdd = 8.0 + (i as f64) * 0.2;
                DailyDataPoint {
                    date: d(2025, 5, 1) + chrono::Duration::days(i),
                    usage: 2.0 * cdd + 30.0,
                    cooling_degree_days: cdd,
                    heating_degree_days: 0.0,
                }
            })
            .collect();

        let model = compute_baseline(&baseline_pts, BaselineMode::Cooling).unwrap();

        // Result period: much hotter (CDD ~20) but intervention saves 15%
        let result_pts: Vec<DailyDataPoint> = (0..30)
            .map(|i| {
                let cdd = 18.0 + (i as f64) * 0.2;
                DailyDataPoint {
                    date: d(2025, 7, 1) + chrono::Duration::days(i),
                    usage: (2.0 * cdd + 30.0) * 0.85,
                    cooling_degree_days: cdd,
                    heating_degree_days: 0.0,
                }
            })
            .collect();

        let eval = evaluate_experiment(&baseline_pts, &result_pts, Some(&model), 0.11).unwrap();

        // Raw change might be negative (usage went up due to heat), but
        // weather-normalized should show ~15% savings.
        let wn = eval.weather_normalized_change_pct.unwrap();
        assert!(
            (wn - 0.15).abs() < 0.02,
            "weather-normalized should show ~15% savings, got {:.2}%",
            wn * 100.0
        );
    }

    #[test]
    fn test_empty_baseline_error() {
        let result = make_data(d(2025, 7, 1), 30, 40.0, 15.0);
        let eval = evaluate_experiment(&[], &result, None, 0.11);
        assert!(eval.is_err());
    }

    #[test]
    fn test_empty_result_error() {
        let baseline = make_data(d(2025, 6, 1), 30, 50.0, 15.0);
        let eval = evaluate_experiment(&baseline, &[], None, 0.11);
        assert!(eval.is_err());
    }

    // --- confidence tests ---

    #[test]
    fn test_confidence_minimum_data() {
        // Under 14 days — very low confidence
        let c = compute_confidence(7, 7, None);
        assert!(c < 0.3, "7-day periods should yield low confidence: {c}");
    }

    #[test]
    fn test_confidence_good_data() {
        // 30 days each, good R²
        let c = compute_confidence(30, 30, Some(0.85));
        assert!(c > 0.8, "30-day + high R² should be high confidence: {c}");
    }

    #[test]
    fn test_confidence_no_model() {
        // 30 days each, no model
        let c = compute_confidence(30, 30, None);
        assert!(c > 0.5, "30-day, no model should be moderate confidence: {c}");
        assert!(c < 0.9, "without model should not be very high: {c}");
    }

    #[test]
    fn test_confidence_poor_r_squared() {
        let c = compute_confidence(30, 30, Some(0.15));
        // Poor R² should drag down the score
        assert!(c < 0.8, "poor R² should limit confidence: {c}");
    }

    #[test]
    fn test_confidence_asymmetric_data() {
        // Good baseline, short result
        let c = compute_confidence(60, 10, Some(0.9));
        assert!(c < 0.8, "short result period should limit confidence: {c}");
    }
}
