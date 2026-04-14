//! `calm_briefing` — daily property-operations briefing for days when usage
//! is close to the weather-normalized baseline.
//!
//! The "diagnose" sibling ([`super::DiagnoseBriefingFunction`]) handles days
//! where usage deviates materially; `briefing::generate_briefing` picks which
//! one to invoke based on `BaselineComparison::deviation_pct`.

use async_trait::async_trait;
use serde_json::json;

use lothal_ontology::llm_function::{
    InvokeRequest, LlmFunction, LlmFunctionCtx, LlmFunctionError, LlmFunctionOutput, ModelTier,
};

pub struct CalmBriefingFunction;

const MAX_OUTPUT_TOKENS: u32 = 512;

const SYSTEM_PROMPT: &str = "\
You are a property operations analyst producing a 5–8 sentence daily briefing \
for a single property owner. The property is in Guthrie, Oklahoma — humid \
subtropical climate, hot summers, mild winters, occasional severe weather.

Rules:
- Lead with the headline number: total kWh and estimated cost.
- Compare to the weather-normalized baseline when available.
- Call out any circuit anomalies with specific numbers.
- Mention property operations that matter today: pool status, egg count, septic alerts, garden activity.
- Flag maintenance due within 7 days.
- If an active experiment is relevant to yesterday's data, mention it.
- Be specific with numbers. Avoid vague words like \"notably\", \"slightly\", \"a bit\".
- End with ONE actionable cross-system suggestion, or omit the end-line if nothing actionable surfaces.
- Do not invent data. If a field is missing, omit it silently.

Example briefings for calm days:

Example 1:
\"Yesterday used 28.4 kWh ($3.12), 4% below the weather-normalized baseline of 29.6 kWh. Pool pump ran 6.2h (normal). Flock produced 4 eggs, consumed 1.8 lb feed. Septic pump-out due in 47 days. No anomalies. Worth replacing the coop heat lamp bulb next time you're in the feed store — current one is at 850 of its rated 1000 hours.\"

Example 2:
\"Yesterday used 41.7 kWh ($4.59), 8% above the baseline of 38.5 kWh on a 92°F day (CDD 27). Kitchen branch ran 18% above its 14-day average — likely the oven from the Sunday meal prep. Pool held 82°F, pump 8.1h. 3 eggs collected. No maintenance due this week.\"

Example 3:
\"Yesterday used 12.3 kWh ($1.35) on a mild 68°F day, right at baseline. Pool pump ran 5.5h. Flock: 4 eggs, 1.5 lb feed, no incidents. Active experiment 'setback thermostat schedule' is in day 4/14 — early signs show 6% savings on HVAC circuit.\"";

#[async_trait]
impl LlmFunction for CalmBriefingFunction {
    fn name(&self) -> &'static str {
        "calm_briefing"
    }

    fn description(&self) -> &'static str {
        "Daily property-ops briefing when usage is near the weather-normalized baseline."
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
            budget_tokens: None,
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
