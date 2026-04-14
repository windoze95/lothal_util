pub mod label;
pub mod signature;

use sqlx::PgPool;
use uuid::Uuid;

use lothal_ontology::llm_function::LlmFunctionRegistry;
use crate::AiError;

/// Run NILM device identification on a circuit's readings.
///
/// Extracts power signatures from the time-series data, classifies them via
/// the `nilm_label` [`LlmFunction`][lothal_ontology::LlmFunction] (Tier::Local),
/// and stores labeled results.
pub async fn identify_devices(
    pool: &PgPool,
    circuit_id: Uuid,
    window_days: u32,
    functions: &LlmFunctionRegistry,
) -> Result<Vec<label::DeviceLabel>, AiError> {
    let signatures =
        signature::extract_signatures(pool, circuit_id, window_days).await?;

    if signatures.is_empty() {
        tracing::info!("No power signatures found for circuit {circuit_id}");
        return Ok(Vec::new());
    }

    tracing::info!(
        "Extracted {} power signatures for circuit {circuit_id}",
        signatures.len()
    );

    let labels = label::classify_signatures(&signatures, circuit_id, functions, pool).await?;

    // Store labels in DB.
    for lbl in &labels {
        store_device_label(pool, lbl).await?;
    }

    Ok(labels)
}

async fn store_device_label(pool: &PgPool, label: &label::DeviceLabel) -> Result<(), AiError> {
    let sig_json = serde_json::to_value(&label.signature)?;

    sqlx::query(
        r#"INSERT INTO device_labels (id, circuit_id, device_kind, confidence,
                                       reasoning, signature, model, is_confirmed, created_at)
           VALUES ($1, $2, $3, $4, $5, $6, $7, false, now())"#,
    )
    .bind(label.id)
    .bind(label.circuit_id)
    .bind(&label.device_kind)
    .bind(label.confidence)
    .bind(&label.reasoning)
    .bind(&sig_json)
    .bind(&label.model)
    .execute(pool)
    .await?;

    Ok(())
}
