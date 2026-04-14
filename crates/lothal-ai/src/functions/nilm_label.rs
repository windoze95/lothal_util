//! `nilm_label` — NILM classifier for a batch of power signatures.
//!
//! This is the first [`LlmFunction`][lothal_ontology::LlmFunction] declared
//! with [`ModelTier::Local`], because residential appliance classification is
//! a narrow task local models handle well. Phase 3 will route this to Ollama
//! while frontier-tier functions stay on Anthropic; in Phase 2 the tier is
//! recorded in the trace but not yet enforced at dispatch.

use async_trait::async_trait;
use serde_json::json;

use lothal_ontology::llm_function::{
    InvokeRequest, LlmFunction, LlmFunctionCtx, LlmFunctionError, LlmFunctionOutput, ModelTier,
};

pub struct NilmLabelFunction;

const MAX_OUTPUT_TOKENS: u32 = 2048;

const SYSTEM_PROMPT: &str = "\
You are a Non-Intrusive Load Monitoring (NILM) expert. Given power signatures \
from a residential electrical circuit, identify the most likely device type.

Common residential device signatures:
- HVAC compressor: 2000-5000W steady/cycling, long duration, correlates with temperature
- Pool pump: 1000-3000W steady, long runs (6-12h), often scheduled
- Water heater: 3500-5500W steady, 15-45min runs, random timing
- Dryer: 2000-5000W steady with cycling, 30-60min
- Oven/range: 2000-5000W variable, during meal times
- Dishwasher: 1200-2400W cycling, evening
- Washing machine: 300-500W variable, 30-60min
- Refrigerator: 100-400W cycling, 15-30min on/off pattern, 24/7
- EV charger: 1400-11500W steady, evening/night, long duration
- Microwave: 1000-1800W steady, 1-10min burst
- Hair dryer: 1000-1800W steady, 5-20min, morning
- Space heater: 500-1500W steady, long runs

Device kind values: hvac, pool_pump, water_heater, dryer, oven, dishwasher, \
washer, refrigerator, ev_charger, microwave, lighting, fan, computer, \
entertainment, small_appliance, unknown

Respond with a confidence from 0.0 to 1.0. Be conservative — if you're unsure, \
use 'unknown' with low confidence and explain what additional data would help.";

#[async_trait]
impl LlmFunction for NilmLabelFunction {
    fn name(&self) -> &'static str {
        "nilm_label"
    }

    fn description(&self) -> &'static str {
        "Classify a batch of residential-circuit power signatures into device kinds."
    }

    fn tier(&self) -> ModelTier {
        ModelTier::Local
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
            "required": ["signatures_prompt", "signature_count"],
            "properties": {
                "signatures_prompt": {
                    "type": "string",
                    "description": "Pre-formatted signature descriptions"
                },
                "signature_count": {
                    "type": "integer",
                    "description": "Number of signatures; schema enforces array length"
                }
            }
        })
    }

    fn output_schema(&self) -> serde_json::Value {
        // Schema is shaped at call time with the actual count; this is the
        // shape sans the array-length bounds.
        classification_schema(0)
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

        let signatures_prompt = input
            .get("signatures_prompt")
            .and_then(|v| v.as_str())
            .ok_or_else(|| LlmFunctionError::InvalidInput("missing `signatures_prompt`".into()))?
            .to_string();
        let count = input
            .get("signature_count")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| LlmFunctionError::InvalidInput("missing `signature_count`".into()))?
            as usize;

        let schema = classification_schema(count);

        let req = InvokeRequest {
            tier: self.tier(),
            system: SYSTEM_PROMPT.to_string(),
            user: signatures_prompt,
            max_tokens: self.max_tokens(),
            budget_tokens: None,
            json_schema: Some(schema),
        };

        let response = invoker.invoke(&req).await.map_err(LlmFunctionError::Other)?;

        Ok(LlmFunctionOutput {
            output: response.content.clone(),
            response,
        })
    }
}

fn classification_schema(count: usize) -> serde_json::Value {
    let mut array_schema = json!({
        "type": "array",
        "items": {
            "type": "object",
            "required": ["device_kind", "confidence", "reasoning"],
            "properties": {
                "device_kind": { "type": "string" },
                "confidence":  { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                "reasoning":   { "type": "string" }
            }
        }
    });
    if count > 0 {
        if let Some(obj) = array_schema.as_object_mut() {
            obj.insert("minItems".into(), json!(count));
            obj.insert("maxItems".into(), json!(count));
        }
    }

    json!({
        "type": "object",
        "required": ["classifications"],
        "properties": {
            "classifications": array_schema
        }
    })
}
