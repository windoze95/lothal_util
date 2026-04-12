use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// An hourly weather observation for a site.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeatherObservation {
    pub time: DateTime<Utc>,
    pub site_id: Uuid,
    pub temperature_f: Option<f64>,
    pub humidity_pct: Option<f64>,
    pub wind_speed_mph: Option<f64>,
    pub wind_direction_deg: Option<f64>,
    pub solar_irradiance_wm2: Option<f64>,
    pub pressure_inhg: Option<f64>,
    pub conditions: Option<String>,
}

impl WeatherObservation {
    pub fn new(site_id: Uuid, time: DateTime<Utc>) -> Self {
        Self {
            time,
            site_id,
            temperature_f: None,
            humidity_pct: None,
            wind_speed_mph: None,
            wind_direction_deg: None,
            solar_irradiance_wm2: None,
            pressure_inhg: None,
            conditions: None,
        }
    }
}

/// Daily weather summary computed from hourly observations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyWeatherSummary {
    pub date: chrono::NaiveDate,
    pub site_id: Uuid,
    pub avg_temp_f: f64,
    pub min_temp_f: f64,
    pub max_temp_f: f64,
    pub avg_humidity_pct: Option<f64>,
    pub total_solar_wh_m2: Option<f64>,
    pub cooling_degree_days: f64,
    pub heating_degree_days: f64,
}

impl DailyWeatherSummary {
    /// Compute from a set of hourly observations for a single day.
    pub fn from_observations(
        date: chrono::NaiveDate,
        site_id: Uuid,
        observations: &[WeatherObservation],
        base_temp_f: f64,
    ) -> Option<Self> {
        let temps: Vec<f64> = observations
            .iter()
            .filter_map(|o| o.temperature_f)
            .collect();
        if temps.is_empty() {
            return None;
        }

        let avg_temp = temps.iter().sum::<f64>() / temps.len() as f64;
        let min_temp = temps.iter().cloned().fold(f64::INFINITY, f64::min);
        let max_temp = temps.iter().cloned().fold(f64::NEG_INFINITY, f64::max);

        let avg_humidity = {
            let h: Vec<f64> = observations
                .iter()
                .filter_map(|o| o.humidity_pct)
                .collect();
            if h.is_empty() {
                None
            } else {
                Some(h.iter().sum::<f64>() / h.len() as f64)
            }
        };

        let cdd = (avg_temp - base_temp_f).max(0.0);
        let hdd = (base_temp_f - avg_temp).max(0.0);

        Some(Self {
            date,
            site_id,
            avg_temp_f: avg_temp,
            min_temp_f: min_temp,
            max_temp_f: max_temp,
            avg_humidity_pct: avg_humidity,
            total_solar_wh_m2: None,
            cooling_degree_days: cdd,
            heating_degree_days: hdd,
        })
    }
}
