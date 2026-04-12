use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::signature::PowerSignature;
use crate::provider::{CompletionRequest, LlmClient, Message, Role};
use crate::AiError;

/// A device label assigned by the LLM to a power signature.
#[derive(Debug, Clone, Serialize)]
pub struct DeviceLabel {
    pub id: Uuid,
    pub circuit_id: Uuid,
    pub device_kind: String,
    pub confidence: f64,
    pub reasoning: String,
    pub signature: PowerSignature,
    pub model: String,
}

/// LLM response for a single signature classification.
#[derive(Debug, Deserialize)]
struct ClassificationResult {
    device_kind: String,
    confidence: f64,
    reasoning: String,
}

#[derive(Debug, Deserialize)]
struct ClassificationResponse {
    classifications: Vec<ClassificationResult>,
}

/// Classify a batch of power signatures using the LLM.
pub async fn classify_signatures(
    signatures: &[PowerSignature],
    circuit_id: Uuid,
    provider: &LlmClient,
) -> Result<Vec<DeviceLabel>, AiError> {
    // Build a summary of signatures for the prompt.
    let sig_descriptions: Vec<String> = signatures
        .iter()
        .enumerate()
        .map(|(i, s)| {
            format!(
                "Signature {}: {:.0}W peak, {:.0}W avg, {:.1} min, pattern={:?}, {}, {}",
                i + 1,
                s.peak_watts,
                s.avg_watts,
                s.duration_minutes,
                s.pattern,
                s.time_of_day,
                s.day_of_week
            )
        })
        .collect();

    let request = CompletionRequest {
        system: NILM_SYSTEM_PROMPT.to_string(),
        messages: vec![Message {
            role: Role::User,
            content: format!(
                "Classify these {} power signatures from a residential circuit:\n\n{}",
                signatures.len(),
                sig_descriptions.join("\n")
            ),
        }],
        max_tokens: 2048,
        temperature: 0.1,
    };

    let schema = classification_schema(signatures.len());
    let raw = provider.complete_json(&request, &schema).await?;

    let response: ClassificationResponse = serde_json::from_value(raw)
        .map_err(|e| AiError::LlmResponse(format!("Failed to parse classification: {e}")))?;

    let model_name = provider.model_name().to_string();

    let labels = response
        .classifications
        .into_iter()
        .zip(signatures.iter())
        .map(|(cls, sig)| DeviceLabel {
            id: Uuid::new_v4(),
            circuit_id,
            device_kind: cls.device_kind,
            confidence: cls.confidence.clamp(0.0, 1.0),
            reasoning: cls.reasoning,
            signature: sig.clone(),
            model: model_name.clone(),
        })
        .collect();

    Ok(labels)
}

const NILM_SYSTEM_PROMPT: &str = "\
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

fn classification_schema(count: usize) -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "required": ["classifications"],
        "properties": {
            "classifications": {
                "type": "array",
                "minItems": count,
                "maxItems": count,
                "items": {
                    "type": "object",
                    "required": ["device_kind", "confidence", "reasoning"],
                    "properties": {
                        "device_kind": { "type": "string" },
                        "confidence": { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                        "reasoning": { "type": "string" }
                    }
                }
            }
        }
    })
}
