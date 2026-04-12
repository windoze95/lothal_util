use crate::IngestError;
use chrono::{DateTime, Utc};
use lothal_core::WeatherObservation;
use tracing::{debug, warn};
use uuid::Uuid;

/// Configuration for the NWS (National Weather Service) API client.
#[derive(Debug, Clone)]
pub struct NwsConfig {
    /// Weather station identifier (e.g. "KGOK").
    pub station_id: String,
    /// Required User-Agent header for the NWS API.
    pub user_agent: String,
}

impl Default for NwsConfig {
    fn default() -> Self {
        Self {
            station_id: "KGOK".into(),
            user_agent: "lothal-ingest/0.1 (github.com/lothal)".into(),
        }
    }
}

/// Build the NWS observations API URL for a given station.
pub fn nws_api_url(station: &str) -> String {
    format!("https://api.weather.gov/stations/{station}/observations")
}

/// Build a `reqwest::Client` with the required NWS User-Agent header.
fn build_client(user_agent: &str) -> Result<reqwest::Client, IngestError> {
    reqwest::Client::builder()
        .user_agent(user_agent)
        .build()
        .map_err(IngestError::Http)
}

/// Fetch the most recent observations from a NWS station.
///
/// `count` controls how many observations to request (the NWS `limit` param).
pub async fn fetch_latest_observations(
    config: &NwsConfig,
    count: usize,
) -> Result<Vec<WeatherObservation>, IngestError> {
    let client = build_client(&config.user_agent)?;
    let url = nws_api_url(&config.station_id);

    debug!(station = %config.station_id, count, "fetching latest NWS observations");

    let resp = client
        .get(&url)
        .query(&[("limit", count.to_string())])
        .header("Accept", "application/geo+json")
        .send()
        .await?
        .error_for_status()?;

    let json: serde_json::Value = resp.json().await?;

    // We don't have a site_id from NWS alone — use a nil UUID. Callers should
    // set the correct site_id after parsing.
    parse_nws_response(&json, Uuid::nil())
}

/// Fetch observations for a specific date range from a NWS station.
pub async fn fetch_observations_range(
    config: &NwsConfig,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Result<Vec<WeatherObservation>, IngestError> {
    let client = build_client(&config.user_agent)?;
    let url = nws_api_url(&config.station_id);

    debug!(
        station = %config.station_id,
        %start,
        %end,
        "fetching NWS observations for date range"
    );

    let resp = client
        .get(&url)
        .query(&[
            ("start", start.to_rfc3339()),
            ("end", end.to_rfc3339()),
        ])
        .header("Accept", "application/geo+json")
        .send()
        .await?
        .error_for_status()?;

    let json: serde_json::Value = resp.json().await?;
    parse_nws_response(&json, Uuid::nil())
}

// ---------------------------------------------------------------------------
// Unit conversions
// ---------------------------------------------------------------------------

/// Celsius to Fahrenheit.
fn celsius_to_fahrenheit(c: f64) -> f64 {
    c * 9.0 / 5.0 + 32.0
}

/// km/h to mph.
fn kmh_to_mph(kmh: f64) -> f64 {
    kmh * 0.621371
}

/// Pascals to inches of mercury.
fn pa_to_inhg(pa: f64) -> f64 {
    pa * 0.000295301
}

// ---------------------------------------------------------------------------
// Response parsing
// ---------------------------------------------------------------------------

/// Extract an optional numeric value from a NWS measurement object.
///
/// NWS properties are typically `{ "value": <number|null>, "unitCode": "..." }`.
/// Returns `None` if the value is null or the key is absent.
fn extract_measurement(obj: &serde_json::Value) -> Option<f64> {
    obj.get("value").and_then(|v| v.as_f64())
}

/// Parse a NWS GeoJSON observations response into [`WeatherObservation`] objects.
///
/// Performs unit conversions from NWS native units to the lothal domain:
/// - Temperature: degC -> degF
/// - Wind speed: km/h -> mph
/// - Pressure: Pa -> inHg
pub fn parse_nws_response(
    json: &serde_json::Value,
    site_id: Uuid,
) -> Result<Vec<WeatherObservation>, IngestError> {
    let features = json
        .get("features")
        .and_then(|f| f.as_array())
        .ok_or_else(|| IngestError::Parse("NWS response missing 'features' array".into()))?;

    let mut observations = Vec::with_capacity(features.len());

    for feature in features {
        let props = match feature.get("properties") {
            Some(p) => p,
            None => {
                warn!("NWS feature missing 'properties', skipping");
                continue;
            }
        };

        // Parse the observation timestamp.
        let timestamp_str = props
            .get("timestamp")
            .and_then(|v| v.as_str())
            .ok_or_else(|| IngestError::Parse("observation missing timestamp".into()))?;

        let time: DateTime<Utc> = DateTime::parse_from_rfc3339(timestamp_str)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| IngestError::Parse(format!("invalid timestamp '{timestamp_str}': {e}")))?;

        let mut obs = WeatherObservation::new(site_id, time);

        // Temperature: degC -> degF
        if let Some(temp_c) = props.get("temperature").and_then(extract_measurement) {
            obs.temperature_f = Some(celsius_to_fahrenheit(temp_c));
        }

        // Relative humidity (already in %)
        if let Some(rh) = props.get("relativeHumidity").and_then(extract_measurement) {
            obs.humidity_pct = Some(rh);
        }

        // Wind speed: km/h -> mph
        if let Some(wind_kmh) = props.get("windSpeed").and_then(extract_measurement) {
            obs.wind_speed_mph = Some(kmh_to_mph(wind_kmh));
        }

        // Wind direction (degrees, no conversion needed)
        if let Some(dir) = props.get("windDirection").and_then(extract_measurement) {
            obs.wind_direction_deg = Some(dir);
        }

        // Barometric pressure: Pa -> inHg
        if let Some(pa) = props.get("barometricPressure").and_then(extract_measurement) {
            obs.pressure_inhg = Some(pa_to_inhg(pa));
        }

        // Text description
        if let Some(desc) = props.get("textDescription").and_then(|v| v.as_str()) {
            obs.conditions = Some(desc.to_string());
        }

        observations.push(obs);
    }

    debug!(count = observations.len(), "parsed NWS observations");
    Ok(observations)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nws_api_url() {
        assert_eq!(
            nws_api_url("KGOK"),
            "https://api.weather.gov/stations/KGOK/observations"
        );
    }

    #[test]
    fn test_unit_conversions() {
        // 0 C = 32 F
        assert!((celsius_to_fahrenheit(0.0) - 32.0).abs() < 0.001);
        // 100 C = 212 F
        assert!((celsius_to_fahrenheit(100.0) - 212.0).abs() < 0.001);
        // 100 km/h ~ 62.1371 mph
        assert!((kmh_to_mph(100.0) - 62.1371).abs() < 0.001);
        // 101325 Pa = ~29.921 inHg (standard atmosphere)
        assert!((pa_to_inhg(101325.0) - 29.921).abs() < 0.01);
    }

    #[test]
    fn test_parse_nws_response() {
        let json: serde_json::Value = serde_json::json!({
            "features": [
                {
                    "properties": {
                        "timestamp": "2026-04-11T12:00:00+00:00",
                        "temperature": { "value": 25.0, "unitCode": "wmoUnit:degC" },
                        "relativeHumidity": { "value": 65.0 },
                        "windSpeed": { "value": 15.0, "unitCode": "wmoUnit:km_h-1" },
                        "windDirection": { "value": 180 },
                        "barometricPressure": { "value": 101325, "unitCode": "wmoUnit:Pa" },
                        "textDescription": "Partly Cloudy"
                    }
                }
            ]
        });

        let site_id = Uuid::new_v4();
        let obs = parse_nws_response(&json, site_id).unwrap();
        assert_eq!(obs.len(), 1);

        let o = &obs[0];
        assert_eq!(o.site_id, site_id);

        // 25 C = 77 F
        assert!((o.temperature_f.unwrap() - 77.0).abs() < 0.01);
        assert!((o.humidity_pct.unwrap() - 65.0).abs() < 0.01);
        // 15 km/h ~ 9.32 mph
        assert!((o.wind_speed_mph.unwrap() - 9.32).abs() < 0.1);
        assert!((o.wind_direction_deg.unwrap() - 180.0).abs() < 0.01);
        // 101325 Pa ~ 29.92 inHg
        assert!((o.pressure_inhg.unwrap() - 29.92).abs() < 0.1);
        assert_eq!(o.conditions.as_deref(), Some("Partly Cloudy"));
    }

    #[test]
    fn test_parse_nws_null_values() {
        let json: serde_json::Value = serde_json::json!({
            "features": [
                {
                    "properties": {
                        "timestamp": "2026-04-11T12:00:00+00:00",
                        "temperature": { "value": null, "unitCode": "wmoUnit:degC" },
                        "relativeHumidity": { "value": null },
                        "windSpeed": { "value": null },
                        "windDirection": { "value": null },
                        "barometricPressure": { "value": null },
                        "textDescription": "Fair"
                    }
                }
            ]
        });

        let obs = parse_nws_response(&json, Uuid::nil()).unwrap();
        assert_eq!(obs.len(), 1);
        let o = &obs[0];
        assert!(o.temperature_f.is_none());
        assert!(o.humidity_pct.is_none());
        assert!(o.wind_speed_mph.is_none());
        assert_eq!(o.conditions.as_deref(), Some("Fair"));
    }
}
