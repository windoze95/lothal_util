//! `ingest_bill_pdf` — stub. Will live here once the PDF-extraction pipeline
//! (currently in `lothal-ai::extract`) is callable from the ontology layer.
//! For now the action is typed so the registry can surface the schema.

use async_trait::async_trait;
use serde_json::json;

use crate::action::{Action, ActionCtx, ActionError};

pub struct IngestBillPdf;

#[async_trait]
impl Action for IngestBillPdf {
    fn name(&self) -> &'static str {
        "ingest_bill_pdf"
    }

    fn description(&self) -> &'static str {
        "Ingest a utility bill PDF for a utility_account. (stub)"
    }

    fn applicable_kinds(&self) -> &'static [&'static str] {
        &["utility_account"]
    }

    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "required": ["pdf_base64"],
            "properties": { "pdf_base64": {"type": "string"} }
        })
    }

    fn output_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": { "bill_id": {"type": "string", "format": "uuid"} }
        })
    }

    async fn run(
        &self,
        _ctx: &ActionCtx,
        _input: serde_json::Value,
    ) -> Result<serde_json::Value, ActionError> {
        Err(ActionError::Other(anyhow::anyhow!(
            "ingest_bill_pdf not yet implemented"
        )))
    }
}
