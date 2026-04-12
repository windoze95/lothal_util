use crate::IngestError;
use chrono::{DateTime, NaiveDate, NaiveDateTime, Utc};
use lothal_core::{Reading, ReadingKind, ReadingSource};
use serde::{Deserialize, Serialize};
use tracing::debug;
use uuid::Uuid;

/// Create a deterministic UUID from a string identifier.
///
/// This produces a UUID v4-shaped value by hashing the input bytes.
/// Used when we need a stable Uuid for a given thermostat ID string.
fn deterministic_uuid(input: &[u8]) -> Uuid {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    input.hash(&mut hasher);
    let hash = hasher.finish();
    let mut hasher2 = DefaultHasher::new();
    (input, 0xDEAD_BEEFu32).hash(&mut hasher2);
    let hash2 = hasher2.finish();

    let mut bytes = [0u8; 16];
    bytes[..8].copy_from_slice(&hash2.to_le_bytes());
    bytes[8..].copy_from_slice(&hash.to_le_bytes());
    bytes[6] = (bytes[6] & 0x0F) | 0x40;
    bytes[8] = (bytes[8] & 0x3F) | 0x80;
    Uuid::from_bytes(bytes)
}

/// Configuration for the Ecobee API.
#[derive(Debug, Clone)]
pub struct EcobeeConfig {
    pub api_key: String,
    pub refresh_token: Option<String>,
}

/// OAuth token for the Ecobee API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EcobeeToken {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: DateTime<Utc>,
}

const ECOBEE_API_BASE: &str = "https://api.ecobee.com";

/// Request an authorization PIN for the Ecobee PIN-based OAuth flow.
///
/// The user must enter the returned PIN at <https://www.ecobee.com/consumerportal/index.html>
/// to authorize the application. Returns `(pin, authorization_code)`.
pub async fn get_pin(api_key: &str) -> Result<(String, String), IngestError> {
    let client = reqwest::Client::new();

    debug!("requesting Ecobee authorization PIN");

    let resp = client
        .get(format!("{ECOBEE_API_BASE}/authorize"))
        .query(&[
            ("response_type", "ecobeePin"),
            ("client_id", api_key),
            ("scope", "smartRead"),
        ])
        .send()
        .await?
        .error_for_status()?;

    let json: serde_json::Value = resp.json().await?;

    let pin = json
        .get("ecobeePin")
        .and_then(|v| v.as_str())
        .ok_or_else(|| IngestError::Parse("missing ecobeePin in response".into()))?
        .to_string();

    let auth_code = json
        .get("code")
        .and_then(|v| v.as_str())
        .ok_or_else(|| IngestError::Parse("missing code in PIN response".into()))?
        .to_string();

    debug!(pin = %pin, "received Ecobee authorization PIN");

    Ok((pin, auth_code))
}

/// Exchange an authorization code for access and refresh tokens.
///
/// Call this after the user has entered the PIN on the Ecobee portal.
pub async fn exchange_code(
    api_key: &str,
    auth_code: &str,
) -> Result<EcobeeToken, IngestError> {
    let client = reqwest::Client::new();

    debug!("exchanging Ecobee authorization code for tokens");

    let resp = client
        .post(format!("{ECOBEE_API_BASE}/token"))
        .form(&[
            ("grant_type", "ecobeePin"),
            ("code", auth_code),
            ("client_id", api_key),
        ])
        .send()
        .await?
        .error_for_status()?;

    parse_token_response(resp).await
}

/// Refresh an expired Ecobee access token.
pub async fn refresh_token(
    api_key: &str,
    refresh: &str,
) -> Result<EcobeeToken, IngestError> {
    let client = reqwest::Client::new();

    debug!("refreshing Ecobee access token");

    let resp = client
        .post(format!("{ECOBEE_API_BASE}/token"))
        .form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh),
            ("client_id", api_key),
        ])
        .send()
        .await?
        .error_for_status()?;

    parse_token_response(resp).await
}

/// Parse a token endpoint response into an [`EcobeeToken`].
async fn parse_token_response(resp: reqwest::Response) -> Result<EcobeeToken, IngestError> {
    #[derive(Deserialize)]
    struct TokenResponse {
        access_token: String,
        refresh_token: String,
        expires_in: i64,
    }

    let token_resp: TokenResponse = resp.json().await?;
    let expires_at = Utc::now() + chrono::Duration::seconds(token_resp.expires_in);

    Ok(EcobeeToken {
        access_token: token_resp.access_token,
        refresh_token: token_resp.refresh_token,
        expires_at,
    })
}

/// Fetch thermostat runtime report data for a date range.
///
/// The Ecobee runtime report includes HVAC runtime data and temperature
/// readings at 5-minute intervals. This function returns [`Reading`] objects
/// for:
/// - [`ReadingKind::RuntimeMinutes`] — cool1, cool2, heat1, heat2 stage runtimes
/// - [`ReadingKind::TemperatureF`] — indoor and outdoor temperatures
///
/// The thermostat_id is used to derive a deterministic [`ReadingSource::Device`].
pub async fn fetch_runtime_report(
    token: &EcobeeToken,
    thermostat_id: &str,
    start: NaiveDate,
    end: NaiveDate,
) -> Result<Vec<Reading>, IngestError> {
    let client = reqwest::Client::new();

    debug!(
        thermostat_id,
        %start,
        %end,
        "fetching Ecobee runtime report"
    );

    // Ecobee requires dates as "YYYY-MM-DD" and the request body as a JSON
    // string in the `body` query parameter.
    let request_body = serde_json::json!({
        "startDate": start.format("%Y-%m-%d").to_string(),
        "endDate": end.format("%Y-%m-%d").to_string(),
        "columns": "compCool1,compCool2,compHeat1,compHeat2,zoneAveTemp,outdoorTemp",
        "selection": {
            "selectionType": "thermostats",
            "selectionMatch": thermostat_id,
        }
    });

    let resp = client
        .get(format!("{ECOBEE_API_BASE}/1/runtimeReport"))
        .query(&[("format", "json"), ("body", &request_body.to_string())])
        .bearer_auth(&token.access_token)
        .send()
        .await?
        .error_for_status()?;

    let json: serde_json::Value = resp.json().await?;

    parse_runtime_report(&json, thermostat_id)
}

/// Column indices in the Ecobee runtime report CSV-style rows.
/// Columns requested: compCool1, compCool2, compHeat1, compHeat2, zoneAveTemp, outdoorTemp
const COL_COOL1: usize = 0;
const COL_COOL2: usize = 1;
const COL_HEAT1: usize = 2;
const COL_HEAT2: usize = 3;
const COL_ZONE_TEMP: usize = 4;
const COL_OUTDOOR_TEMP: usize = 5;

/// Parse an Ecobee runtime report response into [`Reading`] objects.
fn parse_runtime_report(
    json: &serde_json::Value,
    thermostat_id: &str,
) -> Result<Vec<Reading>, IngestError> {
    let report_list = json
        .get("reportList")
        .and_then(|r| r.as_array())
        .ok_or_else(|| IngestError::Parse("missing 'reportList' in Ecobee response".into()))?;

    let device_id = deterministic_uuid(thermostat_id.as_bytes());
    let source = ReadingSource::Device(device_id);

    let mut readings = Vec::new();

    for report in report_list {
        let row_list = match report.get("rowList").and_then(|r| r.as_array()) {
            Some(rows) => rows,
            None => continue,
        };

        for row_val in row_list {
            let row_str = match row_val.as_str() {
                Some(s) => s,
                None => continue,
            };

            // Each row is a CSV string: "date,time,col1,col2,..."
            // e.g. "2026-04-11,08:00:00,5,0,0,0,72.1,65.3"
            let fields: Vec<&str> = row_str.split(',').collect();
            if fields.len() < 8 {
                continue;
            }

            let date_str = fields[0];
            let time_str = fields[1];
            let datetime_str = format!("{date_str} {time_str}");

            let time = NaiveDateTime::parse_from_str(&datetime_str, "%Y-%m-%d %H:%M:%S")
                .map(|ndt| ndt.and_utc())
                .map_err(|e| {
                    IngestError::Parse(format!(
                        "invalid datetime '{datetime_str}' in Ecobee report: {e}"
                    ))
                })?;

            // Data columns start at index 2 (after date and time).
            let data_fields = &fields[2..];

            // Runtime columns: values are in seconds of runtime per 5-minute interval.
            // Convert to minutes.
            let runtime_cols = [
                (COL_COOL1, "cool1"),
                (COL_COOL2, "cool2"),
                (COL_HEAT1, "heat1"),
                (COL_HEAT2, "heat2"),
            ];

            for (col_idx, label) in &runtime_cols {
                if let Some(field) = data_fields.get(*col_idx) {
                    if let Ok(seconds) = field.trim().parse::<f64>() {
                        if seconds > 0.0 {
                            let minutes = seconds / 60.0;
                            let mut reading = Reading::at(
                                time,
                                source,
                                ReadingKind::RuntimeMinutes,
                                minutes,
                            );
                            reading.metadata = Some(serde_json::json!({ "stage": label }));
                            readings.push(reading);
                        }
                    }
                }
            }

            // Temperature columns: Ecobee reports temps in tenths of a degree F.
            let temp_cols = [
                (COL_ZONE_TEMP, "indoor"),
                (COL_OUTDOOR_TEMP, "outdoor"),
            ];

            for (col_idx, label) in &temp_cols {
                if let Some(field) = data_fields.get(*col_idx) {
                    if let Ok(raw_val) = field.trim().parse::<f64>() {
                        // Ecobee reports temperatures in tenths of degrees F.
                        let temp_f = raw_val / 10.0;
                        let mut reading = Reading::at(
                            time,
                            source,
                            ReadingKind::TemperatureF,
                            temp_f,
                        );
                        reading.metadata = Some(serde_json::json!({ "sensor": label }));
                        readings.push(reading);
                    }
                }
            }
        }
    }

    debug!(count = readings.len(), "parsed Ecobee runtime readings");
    Ok(readings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_runtime_report() {
        let json = serde_json::json!({
            "reportList": [
                {
                    "rowList": [
                        "2026-04-11,08:00:00,300,0,0,0,721,653",
                        "2026-04-11,08:05:00,0,0,180,0,718,648"
                    ]
                }
            ]
        });

        let readings = parse_runtime_report(&json, "thermostat-abc").unwrap();

        // First row: cool1=300s (5 min), zone=72.1F, outdoor=65.3F -> 3 readings
        // Second row: heat1=180s (3 min), zone=71.8F, outdoor=64.8F -> 3 readings
        assert_eq!(readings.len(), 6);

        // First reading should be cool1 runtime.
        assert_eq!(readings[0].kind, ReadingKind::RuntimeMinutes);
        assert!((readings[0].value - 5.0).abs() < f64::EPSILON); // 300s / 60 = 5 min

        // Check an indoor temperature reading.
        let indoor = readings.iter().find(|r| {
            r.kind == ReadingKind::TemperatureF
                && r.metadata
                    .as_ref()
                    .and_then(|m| m.get("sensor"))
                    .and_then(|v| v.as_str())
                    == Some("indoor")
        });
        assert!(indoor.is_some());
        assert!((indoor.unwrap().value - 72.1).abs() < 0.01);
    }

    #[test]
    fn test_parse_runtime_report_empty() {
        let json = serde_json::json!({
            "reportList": [
                {
                    "rowList": []
                }
            ]
        });

        let readings = parse_runtime_report(&json, "thermostat-xyz").unwrap();
        assert!(readings.is_empty());
    }

    #[test]
    fn test_device_id_deterministic() {
        let id1 = deterministic_uuid(b"thermostat-abc");
        let id2 = deterministic_uuid(b"thermostat-abc");
        assert_eq!(id1, id2);
    }
}
