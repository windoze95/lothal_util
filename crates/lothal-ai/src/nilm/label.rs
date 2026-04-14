use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use lothal_ontology::llm_function::LlmFunctionRegistry;

use super::signature::PowerSignature;
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

/// Classify a batch of power signatures via the `nilm_label` LLM function.
pub async fn classify_signatures(
    signatures: &[PowerSignature],
    circuit_id: Uuid,
    functions: &LlmFunctionRegistry,
    pool: &PgPool,
) -> Result<Vec<DeviceLabel>, AiError> {
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

    let prompt = format!(
        "Classify these {} power signatures from a residential circuit:\n\n{}",
        signatures.len(),
        sig_descriptions.join("\n")
    );

    let call = functions
        .invoke(
            "nilm_label",
            "nilm",
            pool.clone(),
            serde_json::json!({
                "signatures_prompt": prompt,
                "signature_count": signatures.len(),
            }),
            None,
            None,
        )
        .await
        .map_err(|e| AiError::LlmRequest(format!("nilm_label: {e}")))?;

    let raw = call
        .output
        .as_ref()
        .map(|v| v.0.clone())
        .ok_or_else(|| AiError::LlmResponse("nilm_label returned no output".into()))?;

    let response: ClassificationResponse = serde_json::from_value(raw)
        .map_err(|e| AiError::LlmResponse(format!("Failed to parse classification: {e}")))?;

    let model_name = call.model.clone().unwrap_or_default();

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
