//! `diagnostic` — LLM root-cause hypothesis for a circuit or device.
//!
//! The logic is driven by the `run_diagnostic` [`Action`][crate::action::Action]
//! which gathers readings + anomaly events from the DB and hands them to this
//! function as structured input. The function is purely declarative: prompt,
//! schema, tier, and the LLM call — no DB work.

use async_trait::async_trait;
use serde_json::json;

use super::super::{
    InvokeRequest, LlmFunction, LlmFunctionCtx, LlmFunctionError, LlmFunctionOutput, ModelTier,
};

pub struct DiagnosticFunction;

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
impl LlmFunction for DiagnosticFunction {
    fn name(&self) -> &'static str {
        "diagnostic"
    }

    fn description(&self) -> &'static str {
        "Reason over recent readings and anomalies for a circuit or device; \
         return root-cause hypothesis and cheapest confirming test."
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
                    "description": "Pre-formatted context: subject, readings, anomalies"
                }
            }
        })
    }

    fn output_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "required": ["hypothesis", "confidence", "test"],
            "properties": {
                "hypothesis": {"type": "string"},
                "confidence": {"type": "string", "enum": ["low", "medium", "high"]},
                "test":       {"type": "string"}
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
            json_schema: Some(self.output_schema()),
        };

        let response = invoker.invoke(&req).await.map_err(LlmFunctionError::Other)?;

        Ok(LlmFunctionOutput {
            output: response.content.clone(),
            response,
        })
    }
}
