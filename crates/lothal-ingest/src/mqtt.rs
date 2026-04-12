use crate::IngestError;
use chrono::{DateTime, Utc};
use lothal_core::{Reading, ReadingKind, ReadingSource};
use rumqttc::{AsyncClient, Event, MqttOptions, Packet, QoS};
use serde::Deserialize;
use tracing::{debug, error, info, warn};

/// Configuration for connecting to an MQTT broker.
#[derive(Debug, Clone)]
pub struct MqttConfig {
    pub broker_url: String,
    pub client_id: String,
    pub topics: Vec<String>,
    pub username: Option<String>,
    pub password: Option<String>,
}

/// Maps a Home Assistant entity_id pattern to a reading source and kind.
///
/// The `entity_pattern` is matched as a substring against the MQTT topic or
/// the entity_id embedded in the payload. For example, a pattern of
/// `"sensor.emporia_vue_circuit_1_power"` will match any topic containing
/// that string.
#[derive(Debug, Clone)]
pub struct SensorMapping {
    pub entity_pattern: String,
    pub source: ReadingSource,
    pub kind: ReadingKind,
}

/// Home Assistant MQTT sensor payload format.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct HaSensorPayload {
    state: serde_json::Value,
    #[serde(default)]
    attributes: serde_json::Value,
    #[serde(default)]
    last_updated: Option<String>,
}

/// Find the first mapping whose `entity_pattern` appears in `topic`.
fn find_mapping<'a>(topic: &str, mappings: &'a [SensorMapping]) -> Option<&'a SensorMapping> {
    mappings
        .iter()
        .find(|m| topic.contains(&m.entity_pattern))
}

/// Parse a Home Assistant MQTT sensor message into a [`Reading`].
///
/// HA publishes sensor state in two common formats:
/// 1. JSON object: `{"state": "123.45", "attributes": {...}, "last_updated": "..."}`
/// 2. Raw string value: `"123.45"` or `123.45`
///
/// The function returns `Ok(None)` if the topic does not match any mapping,
/// and `Err` if the payload cannot be parsed.
pub fn parse_ha_sensor_message(
    topic: &str,
    payload: &[u8],
    mappings: &[SensorMapping],
) -> Result<Option<Reading>, IngestError> {
    let mapping = match find_mapping(topic, mappings) {
        Some(m) => m,
        None => return Ok(None),
    };

    let payload_str = std::str::from_utf8(payload)
        .map_err(|e| IngestError::Parse(format!("invalid UTF-8 in MQTT payload: {e}")))?;

    let (value, timestamp) = if let Ok(ha_payload) = serde_json::from_str::<HaSensorPayload>(payload_str) {
        let val = parse_state_value(&ha_payload.state)?;
        let ts = ha_payload
            .last_updated
            .as_deref()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(Utc::now);
        (val, ts)
    } else {
        // Try parsing as a bare numeric value.
        let val: f64 = payload_str
            .trim()
            .parse()
            .map_err(|_| IngestError::Parse(format!("cannot parse payload as number: {payload_str}")))?;
        (val, Utc::now())
    };

    let reading = Reading::at(timestamp, mapping.source, mapping.kind, value);
    Ok(Some(reading))
}

/// Extract a numeric value from the HA state field, which may be a string or number.
fn parse_state_value(state: &serde_json::Value) -> Result<f64, IngestError> {
    match state {
        serde_json::Value::Number(n) => n
            .as_f64()
            .ok_or_else(|| IngestError::Parse("state number is not f64".into())),
        serde_json::Value::String(s) => s
            .trim()
            .parse::<f64>()
            .map_err(|_| IngestError::Parse(format!("cannot parse state string as number: {s}"))),
        other => Err(IngestError::Parse(format!(
            "unexpected state type: {other}"
        ))),
    }
}

/// Parse the MQTT broker URL into (host, port).
fn parse_broker_url(url: &str) -> Result<(String, u16), IngestError> {
    // Accept forms like "mqtt://host:port", "host:port", or just "host".
    let stripped = url
        .strip_prefix("mqtt://")
        .or_else(|| url.strip_prefix("tcp://"))
        .unwrap_or(url);

    if let Some((host, port_str)) = stripped.rsplit_once(':') {
        let port: u16 = port_str
            .parse()
            .map_err(|_| IngestError::Parse(format!("invalid MQTT port: {port_str}")))?;
        Ok((host.to_string(), port))
    } else {
        Ok((stripped.to_string(), 1883))
    }
}

/// Connect to the MQTT broker, subscribe to configured topics, and forward
/// parsed [`Reading`] values through the provided channel.
///
/// This function runs in a loop, automatically reconnecting on disconnect.
/// It only returns if the channel receiver is dropped or an unrecoverable
/// error occurs.
pub async fn run_mqtt_subscriber(
    config: MqttConfig,
    mappings: Vec<SensorMapping>,
    tx: tokio::sync::mpsc::Sender<Reading>,
) -> Result<(), IngestError> {
    let (host, port) = parse_broker_url(&config.broker_url)?;

    let mut mqttoptions = MqttOptions::new(&config.client_id, &host, port);
    mqttoptions.set_keep_alive(std::time::Duration::from_secs(30));

    if let (Some(user), Some(pass)) = (&config.username, &config.password) {
        mqttoptions.set_credentials(user, pass);
    }

    info!(
        broker = %config.broker_url,
        client_id = %config.client_id,
        topics = ?config.topics,
        "starting MQTT subscriber"
    );

    loop {
        let (client, mut eventloop) = AsyncClient::new(mqttoptions.clone(), 10);

        // Subscribe to all configured topics.
        for topic in &config.topics {
            if let Err(e) = client.subscribe(topic, QoS::AtLeastOnce).await {
                error!(topic = %topic, error = %e, "failed to subscribe");
                return Err(IngestError::Mqtt(format!("subscribe failed: {e}")));
            }
            debug!(topic = %topic, "subscribed");
        }

        info!("MQTT event loop started");

        loop {
            match eventloop.poll().await {
                Ok(Event::Incoming(Packet::Publish(publish))) => {
                    let topic = &publish.topic;
                    let payload = &publish.payload;

                    match parse_ha_sensor_message(topic, payload, &mappings) {
                        Ok(Some(reading)) => {
                            debug!(
                                topic = %topic,
                                kind = %reading.kind,
                                value = reading.value,
                                "parsed reading"
                            );
                            if tx.send(reading).await.is_err() {
                                info!("channel closed, shutting down MQTT subscriber");
                                return Ok(());
                            }
                        }
                        Ok(None) => {
                            // No mapping matched this topic; skip.
                        }
                        Err(e) => {
                            warn!(topic = %topic, error = %e, "failed to parse MQTT message");
                        }
                    }
                }
                Ok(_) => {
                    // Other events (connack, suback, pingresp, etc.) -- ignore.
                }
                Err(e) => {
                    error!(error = %e, "MQTT connection error, reconnecting in 5s");
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                    break; // Break inner loop to reconnect.
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn test_mappings() -> Vec<SensorMapping> {
        vec![
            SensorMapping {
                entity_pattern: "sensor.emporia_vue_circuit_1_power".into(),
                source: ReadingSource::Circuit(Uuid::nil()),
                kind: ReadingKind::ElectricWatts,
            },
            SensorMapping {
                entity_pattern: "sensor.indoor_temperature".into(),
                source: ReadingSource::Device(Uuid::nil()),
                kind: ReadingKind::TemperatureF,
            },
        ]
    }

    #[test]
    fn test_parse_ha_json_payload() {
        let mappings = test_mappings();
        let topic = "homeassistant/sensor.emporia_vue_circuit_1_power/state";
        let payload = br#"{"state": "1523.7", "attributes": {}, "last_updated": "2026-04-11T10:00:00+00:00"}"#;

        let result = parse_ha_sensor_message(topic, payload, &mappings).unwrap();
        let reading = result.expect("should match mapping");
        assert_eq!(reading.kind, ReadingKind::ElectricWatts);
        assert!((reading.value - 1523.7).abs() < f64::EPSILON);
    }

    #[test]
    fn test_parse_raw_numeric_payload() {
        let mappings = test_mappings();
        let topic = "homeassistant/sensor.indoor_temperature/state";
        let payload = b"72.5";

        let result = parse_ha_sensor_message(topic, payload, &mappings).unwrap();
        let reading = result.expect("should match mapping");
        assert_eq!(reading.kind, ReadingKind::TemperatureF);
        assert!((reading.value - 72.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_parse_no_matching_topic() {
        let mappings = test_mappings();
        let topic = "homeassistant/sensor.unknown_thing/state";
        let payload = b"42.0";

        let result = parse_ha_sensor_message(topic, payload, &mappings).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_ha_numeric_state() {
        let mappings = test_mappings();
        let topic = "homeassistant/sensor.emporia_vue_circuit_1_power/state";
        let payload = br#"{"state": 800.0, "attributes": {}}"#;

        let result = parse_ha_sensor_message(topic, payload, &mappings).unwrap();
        let reading = result.expect("should match");
        assert!((reading.value - 800.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_parse_broker_url() {
        let (host, port) = parse_broker_url("mqtt://192.168.1.100:1883").unwrap();
        assert_eq!(host, "192.168.1.100");
        assert_eq!(port, 1883);

        let (host, port) = parse_broker_url("mybroker.local").unwrap();
        assert_eq!(host, "mybroker.local");
        assert_eq!(port, 1883);
    }
}
