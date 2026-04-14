//! `diagnose_briefing` — diagnostic daily briefing for days when usage
//! deviates materially (>15%) from the weather-normalized baseline.
//!
//! Uses extended thinking (Anthropic only; budget 4000 tokens) to reason
//! through likely causes before writing the briefing.

use async_trait::async_trait;
use serde_json::json;

use lothal_ontology::llm_function::{
    InvokeRequest, LlmFunction, LlmFunctionCtx, LlmFunctionError, LlmFunctionOutput, ModelTier,
};

pub struct DiagnoseBriefingFunction;

const MAX_OUTPUT_TOKENS: u32 = 1024;
const THINKING_BUDGET: u32 = 4000;

const SYSTEM_PROMPT: &str = "\
You are a property operations analyst investigating a meaningful deviation \
from yesterday's weather-normalized energy baseline. The property is in \
Guthrie, Oklahoma.

Your job: produce a briefing that (a) states the deviation plainly, (b) \
reasons through the 2–3 most likely causes using the circuit-level data, \
weather, and active experiments, and (c) proposes the cheapest next step to \
confirm or rule out the most likely cause.

Rules:
- Lead with the headline: actual vs predicted kWh and the percentage deviation.
- Then diagnose. Be concrete — reference specific circuits and numbers.
- If an active experiment could explain the deviation, mention it before blaming a device.
- Distinguish 'a known cause' (experiment, heat wave, holiday) from 'an unexplained anomaly'.
- For unexplained anomalies, propose the cheapest diagnostic test — not a fix.
- 6–10 sentences. Be specific with numbers. Don't hedge with vague language.

Example diagnose briefing:

\"Yesterday used 52.1 kWh ($5.73), 22% above the weather-normalized baseline of 42.7 kWh on a 96°F day (CDD 31). The pool pump circuit ran 11.3h vs its 6.8h 14-day average — accounts for roughly 4.5 kWh of the 9.4 kWh overage. The HVAC circuit was only 4% high, consistent with the temperature. Active experiment 'pool cover on when not swimming' is suspended this week per your note — likely related. Cheapest check: verify the pool pump schedule in Home Assistant didn't revert; the solar cover being off also roughly doubles evaporative heat loss so the pump may be compensating. If the schedule is correct, the next signal would be a temperature sensor on the return line to see whether the pump is actually cooling.\"";

#[async_trait]
impl LlmFunction for DiagnoseBriefingFunction {
    fn name(&self) -> &'static str {
        "diagnose_briefing"
    }

    fn description(&self) -> &'static str {
        "Diagnostic daily briefing for days with >15% deviation from the baseline; uses extended thinking."
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

    fn budget_tokens(&self) -> Option<u32> {
        Some(THINKING_BUDGET)
    }

    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "required": ["prompt"],
            "properties": {
                "prompt": {"type": "string"}
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
            budget_tokens: self.budget_tokens(),
            json_schema: None,
        };

        let response = invoker.invoke(&req).await.map_err(LlmFunctionError::Other)?;
        let briefing = response
            .content
            .as_str()
            .unwrap_or_default()
            .to_string();

        Ok(LlmFunctionOutput {
            output: json!({ "briefing": briefing }),
            response,
        })
    }
}
