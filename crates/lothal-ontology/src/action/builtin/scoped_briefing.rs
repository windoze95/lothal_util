//! `scoped_briefing` — narrative summary of one entity's ontology slice.
//!
//! Uses the generic `query::get_object_view` to pull the subject, its direct
//! neighbors, and its recent event timeline, then delegates to the
//! `scoped_briefing` [`LlmFunction`][crate::llm_function::LlmFunction] for
//! narrative generation. The returned text is stored as a
//! `briefing_generated` event.

use async_trait::async_trait;
use serde_json::json;

use crate::action::{Action, ActionCtx, ActionError};
use crate::query::{self, ViewOptions};
use crate::{EventSpec, ObjectUri};

use super::{subjects_from_input, truncate};

pub struct ScopedBriefing;

#[async_trait]
impl Action for ScopedBriefing {
    fn name(&self) -> &'static str {
        "scoped_briefing"
    }

    fn description(&self) -> &'static str {
        "Generate a contextualized briefing narrative filtered to the subject \
         entity's graph neighborhood."
    }

    fn applicable_kinds(&self) -> &'static [&'static str] {
        &[
            "site",
            "structure",
            "property_zone",
            "flock",
            "garden_bed",
            "pool",
            "circuit",
            "device",
        ]
    }

    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "neighbor_depth": {"type": "integer", "default": 1},
                "event_limit":    {"type": "integer", "default": 50}
            }
        })
    }

    fn output_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "required": ["briefing", "event_id"],
            "properties": {
                "briefing": {"type": "string"},
                "event_id": {"type": "string", "format": "uuid"}
            }
        })
    }

    async fn run(
        &self,
        ctx: &ActionCtx,
        input: serde_json::Value,
    ) -> Result<serde_json::Value, ActionError> {
        let functions = ctx.llm_functions.as_ref().ok_or_else(|| {
            ActionError::Other(anyhow::anyhow!("LlmFunctionRegistry not configured"))
        })?;

        let subjects = subjects_from_input(&input)?;
        let subject = subjects
            .first()
            .ok_or_else(|| ActionError::InvalidInput("scoped_briefing requires one subject".into()))?;

        let event_limit = input
            .get("event_limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(50) as usize;
        let neighbor_depth = input
            .get("neighbor_depth")
            .and_then(|v| v.as_u64())
            .unwrap_or(1) as u8;

        let uri = ObjectUri::new(&subject.kind, subject.id);
        let view = query::get_object_view(
            &ctx.pool,
            &uri,
            ViewOptions {
                event_limit,
                neighbor_depth,
            },
        )
        .await?;

        let prompt = build_prompt(&view);
        let call = functions
            .invoke(
                "scoped_briefing",
                &ctx.invoked_by,
                ctx.pool.clone(),
                json!({ "prompt": prompt }),
                Some(ctx.run_id),
                None,
            )
            .await
            .map_err(|e| ActionError::Other(anyhow::anyhow!("scoped_briefing function: {e}")))?;

        let briefing = call
            .output
            .as_ref()
            .and_then(|v| v.0.get("briefing"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| ActionError::Other(anyhow::anyhow!("scoped_briefing returned no briefing")))?
            .to_string();

        let event_id = ctx
            .emit_event(EventSpec {
                kind: "briefing_generated".into(),
                site_id: view.object.site_id,
                subjects: subjects.clone(),
                summary: truncate(&briefing, 160),
                severity: Some("info".into()),
                properties: json!({
                    "briefing": briefing,
                    "neighbor_count": view.neighbors.len(),
                    "event_count": view.recent_events.len(),
                }),
                source: "action:scoped_briefing".into(),
            })
            .await?;

        Ok(json!({ "briefing": briefing, "event_id": event_id }))
    }
}

fn build_prompt(view: &query::ObjectView) -> String {
    let mut s = String::new();

    s.push_str(&format!(
        "Subject: {} [{}] — {}\n",
        view.object.kind, view.object.id, view.object.display_name
    ));
    if let Ok(props) = serde_json::to_string_pretty(&view.object.properties.0) {
        s.push_str("Properties:\n");
        s.push_str(&props);
        s.push('\n');
    }

    s.push_str(&format!("\nNeighbors ({}):\n", view.neighbors.len()));
    for (link, obj) in view.neighbors.iter().take(40) {
        s.push_str(&format!(
            "  [{}] {} -> {} ({})\n",
            link.kind, obj.kind, obj.display_name, obj.id
        ));
    }

    s.push_str(&format!("\nRecent events ({}):\n", view.recent_events.len()));
    for ev in view.recent_events.iter().take(40) {
        s.push_str(&format!(
            "  {} [{}] {} — {}\n",
            ev.time.to_rfc3339(),
            ev.kind,
            ev.severity.clone().unwrap_or_else(|| "-".into()),
            ev.summary
        ));
    }

    s
}
