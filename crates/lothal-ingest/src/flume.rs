use crate::IngestError;
use chrono::{DateTime, Utc};
use lothal_core::{Reading, ReadingKind, ReadingSource};
use serde::{Deserialize, Serialize};
use tracing::debug;
use uuid::Uuid;

/// Create a deterministic UUID from a string identifier.
///
/// This produces a UUID v4-shaped value by hashing the input bytes.
/// Used when we need a stable Uuid for a given device/meter ID string.
fn deterministic_uuid(input: &[u8]) -> Uuid {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    input.hash(&mut hasher);
    let hash = hasher.finish();
    // Build a second hash for the upper 64 bits.
    let mut hasher2 = DefaultHasher::new();
    (input, 0xDEAD_BEEFu32).hash(&mut hasher2);
    let hash2 = hasher2.finish();

    let mut bytes = [0u8; 16];
    bytes[..8].copy_from_slice(&hash2.to_le_bytes());
    bytes[8..].copy_from_slice(&hash.to_le_bytes());
    // Set version 4 and variant bits for a valid UUID.
    bytes[6] = (bytes[6] & 0x0F) | 0x40;
    bytes[8] = (bytes[8] & 0x3F) | 0x80;
    Uuid::from_bytes(bytes)
}

/// Configuration for the Flume Water API.
#[derive(Debug, Clone)]
pub struct FlumeConfig {
    pub client_id: String,
    pub client_secret: String,
    pub username: String,
    pub password: String,
}

/// OAuth token for the Flume API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlumeToken {
    pub access_token: String,
    pub token_type: String,
    pub expires_at: DateTime<Utc>,
}

/// A Flume water meter device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlumeDevice {
    pub device_id: String,
    pub location_name: Option<String>,
    pub location_timezone: Option<String>,
}

const FLUME_API_BASE: &str = "https://api.flumewater.com";

/// Authenticate with the Flume API using the password grant flow.
///
/// Returns a [`FlumeToken`] that can be used for subsequent API calls.
pub async fn authenticate(config: &FlumeConfig) -> Result<FlumeToken, IngestError> {
    let client = reqwest::Client::new();

    debug!("authenticating with Flume API");

    let body = serde_json::json!({
        "grant_type": "password",
        "client_id": config.client_id,
        "client_secret": config.client_secret,
        "username": config.username,
        "password": config.password,
    });

    let resp = client
        .post(format!("{FLUME_API_BASE}/oauth/token"))
        .json(&body)
        .send()
        .await?
        .error_for_status()?;

    let json: serde_json::Value = resp.json().await?;

    // Flume returns { "data": [{ "access_token": "...", ... }] }
    let data = json
        .get("data")
        .and_then(|d| d.as_array())
        .and_then(|arr| arr.first())
        .ok_or_else(|| IngestError::Parse("unexpected Flume auth response format".into()))?;

    let access_token = data
        .get("access_token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| IngestError::Parse("missing access_token in Flume response".into()))?
        .to_string();

    let token_type = data
        .get("token_type")
        .and_then(|v| v.as_str())
        .unwrap_or("Bearer")
        .to_string();

    let expires_in = data
        .get("expires_in")
        .and_then(|v| v.as_i64())
        .unwrap_or(3600);

    let expires_at = Utc::now() + chrono::Duration::seconds(expires_in);

    debug!("Flume authentication successful, token expires at {expires_at}");

    Ok(FlumeToken {
        access_token,
        token_type,
        expires_at,
    })
}

/// Fetch the list of Flume devices associated with the authenticated user.
pub async fn get_devices(token: &FlumeToken) -> Result<Vec<FlumeDevice>, IngestError> {
    let client = reqwest::Client::new();

    debug!("fetching Flume devices");

    let resp = client
        .get(format!("{FLUME_API_BASE}/me/devices"))
        .bearer_auth(&token.access_token)
        .send()
        .await?
        .error_for_status()?;

    let json: serde_json::Value = resp.json().await?;

    let data = json
        .get("data")
        .and_then(|d| d.as_array())
        .ok_or_else(|| IngestError::Parse("unexpected Flume devices response format".into()))?;

    let devices: Vec<FlumeDevice> = data
        .iter()
        .filter_map(|d| {
            let device_id = d.get("id")?.to_string().trim_matches('"').to_string();
            let location = d.get("location");
            let location_name = location
                .and_then(|l| l.get("name"))
                .and_then(|v| v.as_str())
                .map(String::from);
            let location_timezone = location
                .and_then(|l| l.get("tz"))
                .and_then(|v| v.as_str())
                .map(String::from);

            Some(FlumeDevice {
                device_id,
                location_name,
                location_timezone,
            })
        })
        .collect();

    debug!(count = devices.len(), "fetched Flume devices");
    Ok(devices)
}

/// Fetch water usage data from a Flume device for a given time range.
///
/// `resolution` controls the granularity of the returned data:
/// - `"MIN"` — per-minute readings
/// - `"HR"` — hourly aggregates
/// - `"DAY"` — daily aggregates
///
/// Returns [`Reading`] objects with [`ReadingKind::WaterGallons`].
pub async fn fetch_usage(
    token: &FlumeToken,
    device_id: &str,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    resolution: &str,
) -> Result<Vec<Reading>, IngestError> {
    let client = reqwest::Client::new();

    debug!(
        device_id,
        %start,
        %end,
        resolution,
        "fetching Flume water usage"
    );

    // Flume expects dates as "YYYY-MM-DD HH:MM:SS" in the device's timezone,
    // but for simplicity we send UTC and let the API handle it.
    let query_body = serde_json::json!({
        "queries": [
            {
                "request_id": "lothal-usage",
                "bucket": resolution,
                "since_datetime": start.format("%Y-%m-%d %H:%M:%S").to_string(),
                "until_datetime": end.format("%Y-%m-%d %H:%M:%S").to_string(),
            }
        ]
    });

    let url = format!("{FLUME_API_BASE}/me/devices/{device_id}/query");

    let resp = client
        .post(&url)
        .bearer_auth(&token.access_token)
        .json(&query_body)
        .send()
        .await?
        .error_for_status()?;

    let json: serde_json::Value = resp.json().await?;

    parse_usage_response(&json, device_id)
}

/// Parse the Flume usage query response into [`Reading`] objects.
fn parse_usage_response(
    json: &serde_json::Value,
    device_id: &str,
) -> Result<Vec<Reading>, IngestError> {
    // Response format: { "data": [{ "lothal-usage": [ { "datetime": "...", "value": 1.23 }, ... ] }] }
    let data = json
        .get("data")
        .and_then(|d| d.as_array())
        .and_then(|arr| arr.first())
        .ok_or_else(|| IngestError::Parse("unexpected Flume usage response format".into()))?;

    let buckets = data
        .get("lothal-usage")
        .and_then(|v| v.as_array())
        .ok_or_else(|| {
            IngestError::Parse("missing 'lothal-usage' key in Flume query response".into())
        })?;

    // Use a deterministic UUID derived from the device_id for the ReadingSource.
    let meter_id = deterministic_uuid(device_id.as_bytes());
    let source = ReadingSource::Meter(meter_id);

    let mut readings = Vec::with_capacity(buckets.len());

    for bucket in buckets {
        let datetime_str = bucket
            .get("datetime")
            .and_then(|v| v.as_str())
            .ok_or_else(|| IngestError::Parse("missing datetime in Flume usage bucket".into()))?;

        let value = bucket
            .get("value")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| IngestError::Parse("missing value in Flume usage bucket".into()))?;

        // Flume datetimes are "YYYY-MM-DD HH:MM:SS"; parse as UTC.
        let time = chrono::NaiveDateTime::parse_from_str(datetime_str, "%Y-%m-%d %H:%M:%S")
            .map(|ndt| ndt.and_utc())
            .map_err(|e| {
                IngestError::Parse(format!(
                    "invalid datetime '{datetime_str}' in Flume response: {e}"
                ))
            })?;

        readings.push(Reading::at(time, source, ReadingKind::WaterGallons, value));
    }

    debug!(count = readings.len(), "parsed Flume usage readings");
    Ok(readings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_usage_response() {
        let json = serde_json::json!({
            "data": [
                {
                    "lothal-usage": [
                        { "datetime": "2026-04-11 08:00:00", "value": 2.5 },
                        { "datetime": "2026-04-11 09:00:00", "value": 1.3 },
                        { "datetime": "2026-04-11 10:00:00", "value": 0.8 }
                    ]
                }
            ]
        });

        let readings = parse_usage_response(&json, "device-123").unwrap();
        assert_eq!(readings.len(), 3);
        assert_eq!(readings[0].kind, ReadingKind::WaterGallons);
        assert!((readings[0].value - 2.5).abs() < f64::EPSILON);
        assert!((readings[1].value - 1.3).abs() < f64::EPSILON);
        assert!((readings[2].value - 0.8).abs() < f64::EPSILON);

        // All readings should share the same meter source.
        let expected_meter = deterministic_uuid(b"device-123");
        assert_eq!(readings[0].source, ReadingSource::Meter(expected_meter));
    }

    #[test]
    fn test_parse_usage_empty() {
        let json = serde_json::json!({
            "data": [
                {
                    "lothal-usage": []
                }
            ]
        });

        let readings = parse_usage_response(&json, "device-456").unwrap();
        assert!(readings.is_empty());
    }
}
