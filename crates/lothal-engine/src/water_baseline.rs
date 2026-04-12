//! Water usage baseline modeling.
//!
//! Parallel to the energy baseline in `baseline.rs`, this computes
//! a linear regression of daily water usage against temperature,
//! since irrigation demand increases with heat.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::EngineError;

/// A daily data point for water baseline regression.
#[derive(Debug, Clone)]
pub struct DailyWaterPoint {
    pub date: NaiveDate,
    /// Daily water usage in gallons.
    pub usage_gallons: f64,
    /// Average temperature for the day (Fahrenheit).
    pub avg_temp_f: f64,
}

/// The result of a water baseline regression.
///
/// Models: `usage = slope * avg_temp_f + intercept`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WaterBaselineModel {
    pub slope: f64,
    pub intercept: f64,
    pub r_squared: f64,
    /// Base water load (gallons/day) independent of temperature.
    pub base_load_gallons_per_day: f64,
    pub data_points_count: usize,
}

/// Compute a water baseline: daily gallons as a function of temperature.
///
/// Higher temperatures drive more irrigation, pool evaporation, and
/// livestock water consumption. The model captures this relationship.
pub fn compute_water_baseline(
    data: &[DailyWaterPoint],
) -> Result<WaterBaselineModel, EngineError> {
    let n = data.len();
    if n < 3 {
        return Err(EngineError::InsufficientData(format!(
            "Need at least 3 data points, got {n}"
        )));
    }

    let n_f = n as f64;
    let sum_x: f64 = data.iter().map(|d| d.avg_temp_f).sum();
    let sum_y: f64 = data.iter().map(|d| d.usage_gallons).sum();
    let sum_xy: f64 = data.iter().map(|d| d.avg_temp_f * d.usage_gallons).sum();
    let sum_xx: f64 = data.iter().map(|d| d.avg_temp_f * d.avg_temp_f).sum();

    let denom = n_f * sum_xx - sum_x * sum_x;
    if denom.abs() < 1e-10 {
        return Err(EngineError::Computation(
            "Zero variance in temperature data".into(),
        ));
    }

    let slope = (n_f * sum_xy - sum_x * sum_y) / denom;
    let intercept = (sum_y - slope * sum_x) / n_f;

    // R²
    let mean_y = sum_y / n_f;
    let ss_tot: f64 = data.iter().map(|d| (d.usage_gallons - mean_y).powi(2)).sum();
    let ss_res: f64 = data
        .iter()
        .map(|d| {
            let predicted = slope * d.avg_temp_f + intercept;
            (d.usage_gallons - predicted).powi(2)
        })
        .sum();

    let r_squared = if ss_tot > 0.0 {
        1.0 - ss_res / ss_tot
    } else {
        0.0
    };

    Ok(WaterBaselineModel {
        slope,
        intercept,
        r_squared,
        base_load_gallons_per_day: intercept.max(0.0),
        data_points_count: n,
    })
}

/// Predict daily water usage for a given temperature.
pub fn predict_water_usage(model: &WaterBaselineModel, avg_temp_f: f64) -> f64 {
    (model.slope * avg_temp_f + model.intercept).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_water_baseline_linear() {
        // Perfect linear: usage = 2 * temp + 50
        let data: Vec<DailyWaterPoint> = (0..10)
            .map(|i| {
                let temp = 60.0 + i as f64 * 5.0;
                DailyWaterPoint {
                    date: NaiveDate::from_ymd_opt(2026, 1, 1 + i).unwrap(),
                    usage_gallons: 2.0 * temp + 50.0,
                    avg_temp_f: temp,
                }
            })
            .collect();

        let model = compute_water_baseline(&data).unwrap();
        assert!((model.slope - 2.0).abs() < 0.01);
        assert!((model.intercept - 50.0).abs() < 0.01);
        assert!((model.r_squared - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_predict_water_usage() {
        let model = WaterBaselineModel {
            slope: 1.5,
            intercept: 30.0,
            r_squared: 0.8,
            base_load_gallons_per_day: 30.0,
            data_points_count: 30,
        };
        let predicted = predict_water_usage(&model, 90.0);
        assert!((predicted - 165.0).abs() < 0.01);
    }

    #[test]
    fn test_insufficient_data() {
        let data = vec![
            DailyWaterPoint {
                date: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                usage_gallons: 100.0,
                avg_temp_f: 70.0,
            },
        ];
        assert!(compute_water_baseline(&data).is_err());
    }
}
