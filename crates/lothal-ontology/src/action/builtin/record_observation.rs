//! `record_observation` — attach a free-text note to one or more entities
//! as an ontology event. No side effects beyond the event row.

use async_trait::async_trait;
use serde_json::json;

use crate::action::{Action, ActionCtx, ActionError};
use crate::EventSpec;

use super::{subjects_from_input, truncate};

pub struct RecordObservation;

/// Longest allowed summary in `events.summary`. The event stores the full
/// text in `properties.text`; the summary is the human-readable preview.
const SUMMARY_MAX_CHARS: usize = 160;

#[async_trait]
impl Action for RecordObservation {
    fn name(&self) -> &'static str {
        "record_observation"
    }

    fn description(&self) -> &'static str {
        "Record a free-text human observation linked to one or more entities."
    }

    fn applicable_kinds(&self) -> &'static [&'static str] {
        &[
            "site",
            "structure",
            "device",
            "circuit",
            "property_zone",
            "flock",
            "garden_bed",
            "pool",
            "utility_account",
            "bill",
            "experiment",
        ]
    }

    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "required": ["text"],
            "properties": {
                "text": {"type": "string"},
                "severity": {
                    "type": "string",
                    "enum": ["info", "notice", "warning"],
                    "default": "info"
                }
            }
        })
    }

    fn output_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "required": ["event_id"],
            "properties": { "event_id": {"type": "string", "format": "uuid"} }
        })
    }

    async fn run(
        &self,
        ctx: &ActionCtx,
        input: serde_json::Value,
    ) -> Result<serde_json::Value, ActionError> {
        let text = input
            .get("text")
            .and_then(|t| t.as_str())
            .ok_or_else(|| ActionError::InvalidInput("text is required".into()))?
            .to_string();

        let severity = input
            .get("severity")
            .and_then(|s| s.as_str())
            .unwrap_or("info")
            .to_string();

        let subjects = subjects_from_input(&input)?;
        if subjects.is_empty() {
            return Err(ActionError::InvalidInput(
                "record_observation requires at least one subject".into(),
            ));
        }

        let summary = truncate(&text, SUMMARY_MAX_CHARS);

        let ev = EventSpec {
            kind: "observation".into(),
            site_id: None,
            subjects,
            summary,
            severity: Some(severity),
            properties: json!({ "text": text, "invoked_by": ctx.invoked_by }),
            source: "action:record_observation".into(),
        };

        let event_id = ctx.emit_event(ev).await?;
        Ok(json!({ "event_id": event_id }))
    }
}
