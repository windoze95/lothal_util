//! `bill_extraction` — structured-field extraction from utility bill text.
//!
//! The `ingest_bill_pdf` [`Action`][crate::action::Action] runs `pdftotext`
//! and hands the raw text to this function. The function returns a JSON
//! object conforming to the bill-fields schema; the action maps that to a
//! typed `Bill` + line items and persists it.

use async_trait::async_trait;
use serde_json::json;

use super::super::{
    InvokeRequest, LlmFunction, LlmFunctionCtx, LlmFunctionError, LlmFunctionOutput, ModelTier,
};

pub struct BillExtractionFunction;

const MAX_OUTPUT_TOKENS: u32 = 2048;

const SYSTEM_PROMPT: &str = "\
You are a utility bill data extraction assistant. Given the raw text extracted \
from a utility bill PDF, extract all structured billing information accurately.

Rules:
- All dates must be in YYYY-MM-DD format.
- All dollar amounts must be numeric (no $ signs).
- Usage amounts must be numeric.
- The line_items amounts MUST sum to total_amount (within $0.02).
- Categorize each line item into the most appropriate category.
- If a charge doesn't clearly fit a category, use 'other'.
- For credits, use negative amounts.
- Extract the provider name exactly as it appears on the bill.";

#[async_trait]
impl LlmFunction for BillExtractionFunction {
    fn name(&self) -> &'static str {
        "bill_extraction"
    }

    fn description(&self) -> &'static str {
        "Extract structured billing fields (period, usage, line items) from utility bill text."
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
            "required": ["pdf_text", "utility_type"],
            "properties": {
                "pdf_text":     {"type": "string"},
                "utility_type": {"type": "string"}
            }
        })
    }

    fn output_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "required": [
                "period_start", "period_end", "statement_date",
                "total_usage", "usage_unit", "total_amount", "line_items"
            ],
            "properties": {
                "period_start":   {"type": "string", "description": "YYYY-MM-DD"},
                "period_end":     {"type": "string", "description": "YYYY-MM-DD"},
                "statement_date": {"type": "string", "description": "YYYY-MM-DD"},
                "total_usage":    {"type": "number"},
                "usage_unit":     {"type": "string", "description": "kWh, therms, or gallons"},
                "total_amount":   {"type": "number"},
                "line_items": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "required": ["description", "category", "amount"],
                        "properties": {
                            "description": {"type": "string"},
                            "category": {
                                "type": "string",
                                "enum": [
                                    "base_charge", "energy_charge", "delivery_charge",
                                    "fuel_cost_adjustment", "demand_charge", "rider_charge",
                                    "tax", "fee", "credit", "other"
                                ]
                            },
                            "amount": {"type": "number"},
                            "usage": {"type": ["number", "null"]},
                            "rate":  {"type": ["number", "null"]}
                        }
                    }
                }
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

        let pdf_text = input
            .get("pdf_text")
            .and_then(|v| v.as_str())
            .ok_or_else(|| LlmFunctionError::InvalidInput("missing `pdf_text`".into()))?;
        let utility_type = input
            .get("utility_type")
            .and_then(|v| v.as_str())
            .ok_or_else(|| LlmFunctionError::InvalidInput("missing `utility_type`".into()))?;

        let user = format!(
            "Extract the billing data from this utility bill ({utility_type}):\n\n\
             ---\n{pdf_text}\n---"
        );

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
