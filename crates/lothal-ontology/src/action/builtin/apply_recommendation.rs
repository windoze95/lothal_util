//! `apply_recommendation` — stub. Returns `not_implemented` until the
//! recommendation persistence layer lands. Kept as a typed registry entry so
//! the web UI can surface the action and show it as "coming soon".

use async_trait::async_trait;
use serde_json::json;

use crate::action::{Action, ActionCtx, ActionError};

pub struct ApplyRecommendation;

#[async_trait]
impl Action for ApplyRecommendation {
    fn name(&self) -> &'static str {
        "apply_recommendation"
    }

    fn description(&self) -> &'static str {
        "Apply a stored recommendation to its subject. (stub — full impl in later task)"
    }

    fn applicable_kinds(&self) -> &'static [&'static str] {
        &["site", "device"]
    }

    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "recommendation_id": {"type": "string", "format": "uuid"}
            }
        })
    }

    fn output_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": { "status": {"type": "string"} }
        })
    }

    async fn run(
        &self,
        _ctx: &ActionCtx,
        _input: serde_json::Value,
    ) -> Result<serde_json::Value, ActionError> {
        Err(ActionError::Other(anyhow::anyhow!(
            "apply_recommendation not yet implemented"
        )))
    }
}
