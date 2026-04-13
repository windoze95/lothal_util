//! `run_diagnostic` — LLM-driven root-cause hypothesis for a circuit or device.
//!
//! Pulls recent readings + anomaly events for the subject, hands both to the
//! injected [`LlmCompleter`], parses a JSON response, and persists a
//! `diagnosis` event.

use async_trait::async_trait;
use serde_json::json;

use crate::action::{Action, ActionCtx, ActionError};
use crate::{query, EventSpec, ObjectUri};

use super::{subjects_from_input, truncate};

pub struct RunDiagnostic;

/// Max rows pulled for the prompt; balances context vs. token cost.
const READINGS_LIMIT: i64 = 500;
const DEFAULT_TIME_RANGE_HOURS: i64 = 72;
const MAX_OUTPUT_TOKENS: u32 = 1024;

const SYSTEM_PROMPT: &str = "\
You are a home-energy diagnostician. Given a circuit or device, its recent \
readings, and any anomaly events, produce the single most likely root-cause \
hypothesis and the cheapest test that would confirm or rule it out.

Respond ONLY with a JSON object matching this shape:
{\"hypothesis\": \"...\", \"confidence\": \"low\"|\"medium\"|\"high\", \"test\": \"...\"}

Rules:
- Be concrete and reference specific numbers.
- Prefer tests that need no new hardware.
- If the data is too sparse to reason, return confidence \"low\" and propose the cheapest monitoring step.";

#[async_trait]
impl Action for RunDiagnostic {
    fn name(&self) -> &'static str {
        "run_diagnostic"
    }

    fn description(&self) -> &'static str {
        "Reason over recent readings and anomalies for a circuit or device. \
         Returns root-cause hypothesis and cheapest confirming test."
    }

    fn applicable_kinds(&self) -> &'static [&'static str] {
        &["circuit", "device"]
    }

    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "time_range_hours": {"type": "integer", "default": 72}
            }
        })
    }

    fn output_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "required": ["hypothesis", "confidence", "test", "event_id"],
            "properties": {
                "hypothesis": {"type": "string"},
                "confidence": {"type": "string", "enum": ["low", "medium", "high"]},
                "test": {"type": "string"},
                "event_id": {"type": "string", "format": "uuid"}
            }
        })
    }

    async fn run(
        &self,
        ctx: &ActionCtx,
        input: serde_json::Value,
    ) -> Result<serde_json::Value, ActionError> {
        let llm = ctx
            .llm
            .as_ref()
            .ok_or_else(|| ActionError::Other(anyhow::anyhow!("Claude client not configured")))?;

        let subjects = subjects_from_input(&input)?;
        let subject = subjects
            .first()
            .ok_or_else(|| ActionError::InvalidInput("run_diagnostic requires one subject".into()))?;

        let hours = input
            .get("time_range_hours")
            .and_then(|v| v.as_i64())
            .unwrap_or(DEFAULT_TIME_RANGE_HOURS)
            .max(1);

        let now = chrono::Utc::now();
        let t0 = now - chrono::Duration::hours(hours);

        // `source_type` matches the ontology kind by convention.
        let reading_rows: Vec<(chrono::DateTime<chrono::Utc>, String, f64)> = sqlx::query_as(
            r#"SELECT time, kind, value FROM readings
               WHERE source_type = $1 AND source_id = $2 AND time >= $3 AND time < $4
               ORDER BY time DESC LIMIT $5"#,
        )
        .bind(&subject.kind)
        .bind(subject.id)
        .bind(t0)
        .bind(now)
        .bind(READINGS_LIMIT)
        .fetch_all(&ctx.pool)
        .await?;

        let uri = ObjectUri::new(&subject.kind, subject.id);
        let anomaly_events = query::events_for(&ctx.pool, &[uri], t0, now, Some("anomaly")).await?;
        let prompt = build_prompt(&subject.kind, subject.id, &reading_rows, &anomaly_events, hours);

        // Trait impl is responsible for schema enforcement (tool-use on
        // Anthropic, prompt-injection for local LLMs).
        let response = llm
            .complete_json(SYSTEM_PROMPT, &prompt, MAX_OUTPUT_TOKENS, &self.output_schema())
            .await
            .map_err(ActionError::Other)?;

        let hypothesis = required_response_str(&response, "hypothesis")?;
        let test = required_response_str(&response, "test")?;
        let confidence = response
            .get("confidence")
            .and_then(|v| v.as_str())
            .unwrap_or("low")
            .to_string();

        let event_id = ctx
            .emit_event(EventSpec {
                kind: "diagnosis".into(),
                site_id: None,
                subjects: subjects.clone(),
                summary: truncate(&hypothesis, 160),
                severity: Some(severity_from_confidence(&confidence)),
                properties: json!({
                    "hypothesis": hypothesis,
                    "confidence": confidence,
                    "test": test,
                    "time_range_hours": hours,
                    "reading_count": reading_rows.len(),
                    "anomaly_count": anomaly_events.len(),
                }),
                source: "action:run_diagnostic".into(),
            })
            .await?;

        Ok(json!({
            "hypothesis": hypothesis,
            "confidence": confidence,
            "test": test,
            "event_id": event_id,
        }))
    }
}

fn required_response_str(response: &serde_json::Value, key: &str) -> Result<String, ActionError> {
    response
        .get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| ActionError::Other(anyhow::anyhow!("LLM response missing `{key}`")))
}

fn build_prompt(
    kind: &str,
    id: uuid::Uuid,
    readings: &[(chrono::DateTime<chrono::Utc>, String, f64)],
    anomalies: &[crate::EventRecord],
    hours: i64,
) -> String {
    use std::fmt::Write;
    let mut s = format!("Subject: {kind} {id}\nWindow: last {hours}h\n\n");
    let _ = writeln!(s, "Readings ({} rows, most recent first):", readings.len());
    for (t, k, v) in readings.iter().take(60) {
        let _ = writeln!(s, "  {} {k}={v}", t.to_rfc3339());
    }
    if readings.len() > 60 {
        let _ = writeln!(s, "  ... ({} more truncated)", readings.len() - 60);
    }
    let _ = writeln!(s, "\nAnomaly events ({}):", anomalies.len());
    for ev in anomalies.iter().take(20) {
        let sev = ev.severity.as_deref().unwrap_or("-");
        let _ = writeln!(s, "  {} [{}] {}", ev.time.to_rfc3339(), sev, ev.summary);
    }
    s
}

fn severity_from_confidence(c: &str) -> String {
    match c {
        "high" => "warning",
        "medium" => "notice",
        _ => "info",
    }
    .to_string()
}
