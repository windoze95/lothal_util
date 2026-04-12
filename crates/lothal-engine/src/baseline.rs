//! Weather-normalized baseline computation using simple linear regression.
//!
//! Models daily energy usage as a function of cooling or heating degree days,
//! producing a weather-normalized baseline that separates weather-dependent
//! load from base load.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::EngineError;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Whether the baseline models cooling or heating behaviour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BaselineMode {
    Cooling,
    Heating,
}

/// A single day of paired usage + weather data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyDataPoint {
    pub date: NaiveDate,
    /// Daily usage in kWh (electric) or therms (gas).
    pub usage: f64,
    pub cooling_degree_days: f64,
    pub heating_degree_days: f64,
}

/// Linear‐regression baseline: usage = slope * degree_days + intercept.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaselineModel {
    pub slope: f64,
    pub intercept: f64,
    pub r_squared: f64,
    /// The intercept represents base load — usage when degree days = 0.
    pub base_load_kwh_per_day: f64,
    pub data_points_count: usize,
    /// A human label like "Summer 2025 cooling baseline".
    pub metric_label: String,
}

/// One day's actual vs predicted comparison.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalizedDay {
    pub date: NaiveDate,
    pub actual_usage: f64,
    pub predicted_usage: f64,
    pub residual: f64,
}

/// High-level summary of a baseline period.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaselineSummary {
    pub model: BaselineModel,
    pub total_actual: f64,
    pub total_predicted: f64,
    pub avg_daily_actual: f64,
    pub avg_daily_predicted: f64,
    pub period_description: String,
}

// ---------------------------------------------------------------------------
// Core functions
// ---------------------------------------------------------------------------

/// Fit a simple linear regression of daily usage against degree days.
///
/// The `mode` selects which degree-day column to use as the independent
/// variable (cooling or heating).
///
/// Returns an error if fewer than 3 data points are supplied, or if the
/// independent variable has zero variance (all identical).
pub fn compute_baseline(
    data: &[DailyDataPoint],
    mode: BaselineMode,
) -> Result<BaselineModel, EngineError> {
    if data.len() < 3 {
        return Err(EngineError::InsufficientData(format!(
            "Need at least 3 data points, got {}",
            data.len()
        )));
    }

    let n = data.len() as f64;

    // x = degree days (cooling or heating), y = usage
    let xs: Vec<f64> = data
        .iter()
        .map(|d| match mode {
            BaselineMode::Cooling => d.cooling_degree_days,
            BaselineMode::Heating => d.heating_degree_days,
        })
        .collect();
    let ys: Vec<f64> = data.iter().map(|d| d.usage).collect();

    let x_mean = xs.iter().sum::<f64>() / n;
    let y_mean = ys.iter().sum::<f64>() / n;

    let mut ss_xy = 0.0;
    let mut ss_xx = 0.0;
    for i in 0..data.len() {
        let dx = xs[i] - x_mean;
        let dy = ys[i] - y_mean;
        ss_xy += dx * dy;
        ss_xx += dx * dx;
    }

    if ss_xx.abs() < f64::EPSILON {
        return Err(EngineError::Computation(
            "Zero variance in degree-day data — cannot fit regression".into(),
        ));
    }

    let slope = ss_xy / ss_xx;
    let intercept = y_mean - slope * x_mean;

    // R²  =  1 - SS_res / SS_tot
    let ss_tot: f64 = ys.iter().map(|y| (y - y_mean).powi(2)).sum();
    let ss_res: f64 = xs
        .iter()
        .zip(ys.iter())
        .map(|(x, y)| {
            let predicted = slope * x + intercept;
            (y - predicted).powi(2)
        })
        .sum();

    let r_squared = if ss_tot.abs() < f64::EPSILON {
        // All y values identical — perfect (trivial) fit.
        1.0
    } else {
        1.0 - ss_res / ss_tot
    };

    let label = match mode {
        BaselineMode::Cooling => "Cooling baseline",
        BaselineMode::Heating => "Heating baseline",
    };

    Ok(BaselineModel {
        slope,
        intercept,
        r_squared,
        base_load_kwh_per_day: intercept,
        data_points_count: data.len(),
        metric_label: label.to_string(),
    })
}

/// Predict daily usage from a degree-day value.
pub fn predict_usage(model: &BaselineModel, degree_days: f64) -> f64 {
    model.slope * degree_days + model.intercept
}

/// Compare actual usage to predicted usage for every day in `actual`.
pub fn compute_normalized_usage(
    model: &BaselineModel,
    actual: &[DailyDataPoint],
) -> Vec<NormalizedDay> {
    actual
        .iter()
        .map(|d| {
            let dd = d.cooling_degree_days.max(d.heating_degree_days);
            let predicted = predict_usage(model, dd);
            NormalizedDay {
                date: d.date,
                actual_usage: d.usage,
                predicted_usage: predicted,
                residual: d.usage - predicted,
            }
        })
        .collect()
}

/// Produce a human-readable summary for a baseline period.
pub fn summarize_baseline(
    model: &BaselineModel,
    data: &[DailyDataPoint],
    label: &str,
) -> BaselineSummary {
    let total_actual: f64 = data.iter().map(|d| d.usage).sum();
    let n = data.len().max(1) as f64;
    let avg_daily_actual = total_actual / n;

    let total_predicted: f64 = data
        .iter()
        .map(|d| {
            let dd = d.cooling_degree_days.max(d.heating_degree_days);
            predict_usage(model, dd)
        })
        .sum();
    let avg_daily_predicted = total_predicted / n;

    BaselineSummary {
        model: model.clone(),
        total_actual,
        total_predicted,
        avg_daily_actual,
        avg_daily_predicted,
        period_description: label.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    /// Build a known dataset: usage = 2.0 * CDD + 30.0 (perfect linear).
    fn perfect_cooling_data() -> Vec<DailyDataPoint> {
        vec![
            DailyDataPoint { date: d(2025, 7, 1),  usage: 30.0,  cooling_degree_days: 0.0,  heating_degree_days: 0.0 },
            DailyDataPoint { date: d(2025, 7, 2),  usage: 40.0,  cooling_degree_days: 5.0,  heating_degree_days: 0.0 },
            DailyDataPoint { date: d(2025, 7, 3),  usage: 50.0,  cooling_degree_days: 10.0, heating_degree_days: 0.0 },
            DailyDataPoint { date: d(2025, 7, 4),  usage: 60.0,  cooling_degree_days: 15.0, heating_degree_days: 0.0 },
            DailyDataPoint { date: d(2025, 7, 5),  usage: 70.0,  cooling_degree_days: 20.0, heating_degree_days: 0.0 },
        ]
    }

    #[test]
    fn test_perfect_linear_regression() {
        let data = perfect_cooling_data();
        let model = compute_baseline(&data, BaselineMode::Cooling).unwrap();

        assert!((model.slope - 2.0).abs() < 1e-10, "slope should be 2.0, got {}", model.slope);
        assert!((model.intercept - 30.0).abs() < 1e-10, "intercept should be 30.0, got {}", model.intercept);
        assert!((model.r_squared - 1.0).abs() < 1e-10, "R² should be 1.0, got {}", model.r_squared);
        assert_eq!(model.data_points_count, 5);
    }

    #[test]
    fn test_predict_usage() {
        let data = perfect_cooling_data();
        let model = compute_baseline(&data, BaselineMode::Cooling).unwrap();

        assert!((predict_usage(&model, 0.0) - 30.0).abs() < 1e-10);
        assert!((predict_usage(&model, 12.5) - 55.0).abs() < 1e-10);
        assert!((predict_usage(&model, 25.0) - 80.0).abs() < 1e-10);
    }

    #[test]
    fn test_insufficient_data() {
        let data = vec![
            DailyDataPoint { date: d(2025, 7, 1), usage: 30.0, cooling_degree_days: 0.0, heating_degree_days: 0.0 },
            DailyDataPoint { date: d(2025, 7, 2), usage: 40.0, cooling_degree_days: 5.0, heating_degree_days: 0.0 },
        ];
        let result = compute_baseline(&data, BaselineMode::Cooling);
        assert!(result.is_err());
    }

    #[test]
    fn test_zero_variance_degree_days() {
        let data = vec![
            DailyDataPoint { date: d(2025, 7, 1), usage: 30.0, cooling_degree_days: 5.0, heating_degree_days: 0.0 },
            DailyDataPoint { date: d(2025, 7, 2), usage: 40.0, cooling_degree_days: 5.0, heating_degree_days: 0.0 },
            DailyDataPoint { date: d(2025, 7, 3), usage: 35.0, cooling_degree_days: 5.0, heating_degree_days: 0.0 },
        ];
        let result = compute_baseline(&data, BaselineMode::Cooling);
        assert!(result.is_err());
    }

    #[test]
    fn test_heating_mode() {
        // usage = 1.5 * HDD + 10.0
        let data = vec![
            DailyDataPoint { date: d(2025, 1, 1), usage: 10.0,  cooling_degree_days: 0.0, heating_degree_days: 0.0  },
            DailyDataPoint { date: d(2025, 1, 2), usage: 25.0,  cooling_degree_days: 0.0, heating_degree_days: 10.0 },
            DailyDataPoint { date: d(2025, 1, 3), usage: 40.0,  cooling_degree_days: 0.0, heating_degree_days: 20.0 },
            DailyDataPoint { date: d(2025, 1, 4), usage: 55.0,  cooling_degree_days: 0.0, heating_degree_days: 30.0 },
        ];
        let model = compute_baseline(&data, BaselineMode::Heating).unwrap();

        assert!((model.slope - 1.5).abs() < 1e-10, "slope: {}", model.slope);
        assert!((model.intercept - 10.0).abs() < 1e-10, "intercept: {}", model.intercept);
        assert!((model.r_squared - 1.0).abs() < 1e-10, "R²: {}", model.r_squared);
    }

    #[test]
    fn test_noisy_regression() {
        // Not perfectly linear, but should still produce a reasonable fit.
        let data = vec![
            DailyDataPoint { date: d(2025, 7, 1),  usage: 32.0,  cooling_degree_days: 0.0,  heating_degree_days: 0.0 },
            DailyDataPoint { date: d(2025, 7, 2),  usage: 38.0,  cooling_degree_days: 5.0,  heating_degree_days: 0.0 },
            DailyDataPoint { date: d(2025, 7, 3),  usage: 52.0,  cooling_degree_days: 10.0, heating_degree_days: 0.0 },
            DailyDataPoint { date: d(2025, 7, 4),  usage: 58.0,  cooling_degree_days: 15.0, heating_degree_days: 0.0 },
            DailyDataPoint { date: d(2025, 7, 5),  usage: 72.0,  cooling_degree_days: 20.0, heating_degree_days: 0.0 },
        ];
        let model = compute_baseline(&data, BaselineMode::Cooling).unwrap();

        // Slope should be close to 2.0, intercept close to 30
        assert!((model.slope - 2.0).abs() < 0.3);
        assert!((model.intercept - 30.0).abs() < 3.0);
        assert!(model.r_squared > 0.98);
    }

    #[test]
    fn test_normalized_usage() {
        let data = perfect_cooling_data();
        let model = compute_baseline(&data, BaselineMode::Cooling).unwrap();
        let normalized = compute_normalized_usage(&model, &data);

        assert_eq!(normalized.len(), 5);
        for day in &normalized {
            assert!(day.residual.abs() < 1e-10, "residual should be ~0 for perfect data");
        }
    }

    #[test]
    fn test_summarize_baseline() {
        let data = perfect_cooling_data();
        let model = compute_baseline(&data, BaselineMode::Cooling).unwrap();
        let summary = summarize_baseline(&model, &data, "Test period");

        assert_eq!(summary.period_description, "Test period");
        assert!((summary.total_actual - summary.total_predicted).abs() < 1e-6);
        assert!((summary.avg_daily_actual - 50.0).abs() < 1e-10);
    }
}
