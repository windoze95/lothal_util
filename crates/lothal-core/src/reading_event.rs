//! Broadcast event representing a single live reading observation.
//!
//! This type is intentionally flat and `Clone`-cheap so it can be shipped
//! through a `tokio::sync::broadcast` channel between the MQTT ingester and
//! any number of subscribers (WebSocket clients, dashboards, etc.).
//!
//! The fields intentionally use string forms for `source_kind` and `kind`
//! rather than full ontology enums to keep this type a leaf dependency and
//! easy to serialize as JSON over the wire.
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A lightweight, broadcast-friendly snapshot of one reading.
///
/// Produced by the MQTT ingester on every successful INSERT and consumed by
/// WebSocket clients that want a live feed of sensor activity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadingEvent {
    /// When the reading was taken (UTC).
    pub time: DateTime<Utc>,
    /// The source kind (e.g. `"device"`, `"circuit"`, `"meter"`).
    pub source_kind: String,
    /// The source's UUID.
    pub source_id: Uuid,
    /// The reading kind (e.g. `"electric_watts"`, `"temperature_f"`).
    pub kind: String,
    /// Numeric value of the reading.
    pub value: f64,
}

impl ReadingEvent {
    /// Build a `ReadingEvent` from a `Reading`.
    pub fn from_reading(reading: &crate::ontology::reading::Reading) -> Self {
        Self {
            time: reading.time,
            source_kind: reading.source.source_type().to_string(),
            source_id: reading.source.source_id(),
            kind: reading.kind.as_str().to_string(),
            value: reading.value,
        }
    }

    /// Return the `lothal://{source_kind}/{source_id}` URI of this event's source.
    pub fn uri(&self) -> String {
        format!("lothal://{}/{}", self.source_kind, self.source_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ontology::reading::{Reading, ReadingKind, ReadingSource};

    #[test]
    fn from_reading_and_uri_roundtrip() {
        let id = Uuid::new_v4();
        let r = Reading::new(ReadingSource::Circuit(id), ReadingKind::ElectricWatts, 123.4);
        let evt = ReadingEvent::from_reading(&r);
        assert_eq!(evt.source_kind, "circuit");
        assert_eq!(evt.source_id, id);
        assert_eq!(evt.kind, "electric_watts");
        assert!((evt.value - 123.4).abs() < f64::EPSILON);
        assert_eq!(evt.uri(), format!("lothal://circuit/{}", id));
    }
}
