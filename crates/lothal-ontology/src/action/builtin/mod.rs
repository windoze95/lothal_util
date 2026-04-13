//! Default action implementations shipped with the ontology layer.
//!
//! Each action is a small unit that reads `input._subjects` (injected by the
//! registry), does its work, and emits a matching event. See the individual
//! files for per-action specifics.

pub mod run_diagnostic;
pub mod schedule_maintenance;
pub mod apply_recommendation;
pub mod scoped_briefing;
pub mod ingest_bill_pdf;
pub mod record_observation;

use crate::ObjectRef;
use crate::action::ActionError;

/// Shorten `s` to at most `max` characters, appending a single-glyph ellipsis
/// when the string is cut. Used for `events.summary` fields that must fit a
/// human-readable column.
pub(crate) fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// Read the `_subjects` array the registry injected into `input`.
///
/// Returns an empty vec if the key is missing so callers that tolerate
/// zero subjects (none currently) don't need to special-case.
pub(crate) fn subjects_from_input(input: &serde_json::Value) -> Result<Vec<ObjectRef>, ActionError> {
    let arr = match input.get("_subjects") {
        Some(serde_json::Value::Array(a)) => a,
        Some(_) => {
            return Err(ActionError::InvalidInput(
                "_subjects must be an array".into(),
            ));
        }
        None => return Ok(Vec::new()),
    };

    arr.iter()
        .map(|v| {
            let kind = v
                .get("kind")
                .and_then(|k| k.as_str())
                .ok_or_else(|| ActionError::InvalidInput("subject missing kind".into()))?
                .to_string();
            let id_str = v
                .get("id")
                .and_then(|i| i.as_str())
                .ok_or_else(|| ActionError::InvalidInput("subject missing id".into()))?;
            let id = uuid::Uuid::parse_str(id_str)
                .map_err(|e| ActionError::InvalidInput(format!("subject id parse: {e}")))?;
            Ok(ObjectRef::new(kind, id))
        })
        .collect()
}
