//! `scoped_briefing` — narrative summary of one entity's ontology slice.
//!
//! The `scoped_briefing` [`Action`][crate::action::Action] assembles the
//! ontology view (subject, neighbors, recent events) and hands the
//! pre-formatted prompt to this function. The function is purely declarative:
//! it owns the system prompt, token budget, tier, and the LLM call.

use async_trait::async_trait;
use serde_json::json;

use super::super::{
    InvokeRequest, LlmFunction, LlmFunctionCtx, LlmFunctionError, LlmFunctionOutput, ModelTier,
};

pub struct ScopedBriefingFunction;

const MAX_OUTPUT_TOKENS: u32 = 512;

const SYSTEM_PROMPT: &str = "\
You are a homestead ops briefing. Given the ontology slice below, produce a \
4–6 sentence briefing about the subject, its neighbors, and recent events. \
Lead with anything unusual. End with one actionable observation.";

#[async_trait]
impl LlmFunction for ScopedBriefingFunction {
    fn name(&self) -> &'static str {
        "scoped_briefing"
    }

    fn description(&self) -> &'static str {
        "Narrative briefing over an entity's graph neighborhood and recent events."
    }

    fn tier(&self) -> ModelTier {
        ModelTier::Frontier
    }

    fn system_prompt(&self) -> &str {
        SYSTEM_PROMPT
    }

    fn max_tokens(&self) -> u32 {
        MAX_OUTPUT_TOKENS
    }

    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "required": ["prompt"],
            "properties": {
                "prompt": {
                    "type": "string",
                    "description": "Pre-formatted ontology slice"
                }
            }
        })
    }

    fn output_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "required": ["briefing"],
            "properties": {
                "briefing": {"type": "string"}
            }
        })
    }

    async fn run(
        &self,
        ctx: &LlmFunctionCtx,
        input: serde_json::Value,
    ) -> Result<LlmFunctionOutput, LlmFunctionError> {
        let invoker = ctx
            .invoker
            .as_ref()
            .ok_or(LlmFunctionError::NoInvoker)?;

        let user = input
            .get("prompt")
            .and_then(|v| v.as_str())
            .ok_or_else(|| LlmFunctionError::InvalidInput("missing `prompt`".into()))?
            .to_string();

        let req = InvokeRequest {
            tier: self.tier(),
            system: SYSTEM_PROMPT.to_string(),
            user,
            max_tokens: self.max_tokens(),
            budget_tokens: None,
            // Text response; the output wraps the string under `briefing`.
            json_schema: None,
        };

        let response = invoker.invoke(&req).await.map_err(LlmFunctionError::Other)?;

        // `InvokeResponse::content` for text requests is Value::String(text).
        let briefing = response
            .content
            .as_str()
            .unwrap_or_default()
            .to_string();
        let output = json!({ "briefing": briefing });

        Ok(LlmFunctionOutput { output, response })
    }
}
