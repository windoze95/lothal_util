use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::provider::{CompletionRequest, Message, Role};

/// The structured output the LLM produces when extracting a bill.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedBill {
    pub provider_name: String,
    pub period_start: String,
    pub period_end: String,
    pub statement_date: String,
    pub total_usage: f64,
    pub usage_unit: String,
    pub total_amount: f64,
    pub line_items: Vec<ExtractedLineItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedLineItem {
    pub description: String,
    pub category: String,
    pub amount: f64,
    #[serde(default)]
    pub usage: Option<f64>,
    #[serde(default)]
    pub rate: Option<f64>,
}

/// Build the JSON schema that the LLM must conform to.
pub fn bill_json_schema() -> serde_json::Value {
    json!({
        "type": "object",
        "required": [
            "provider_name", "period_start", "period_end", "statement_date",
            "total_usage", "usage_unit", "total_amount", "line_items"
        ],
        "properties": {
            "provider_name": {
                "type": "string",
                "description": "Name of the utility provider (e.g., 'OG&E', 'ONG', 'City of Guthrie')"
            },
            "period_start": {
                "type": "string",
                "description": "Billing period start date in YYYY-MM-DD format"
            },
            "period_end": {
                "type": "string",
                "description": "Billing period end date in YYYY-MM-DD format"
            },
            "statement_date": {
                "type": "string",
                "description": "Statement date in YYYY-MM-DD format"
            },
            "total_usage": {
                "type": "number",
                "description": "Total usage amount (e.g., kWh for electric, therms for gas, gallons for water)"
            },
            "usage_unit": {
                "type": "string",
                "description": "Unit of usage: 'kWh', 'therms', or 'gallons'"
            },
            "total_amount": {
                "type": "number",
                "description": "Total dollar amount billed"
            },
            "line_items": {
                "type": "array",
                "description": "Individual charges on the bill",
                "items": {
                    "type": "object",
                    "required": ["description", "category", "amount"],
                    "properties": {
                        "description": {
                            "type": "string",
                            "description": "Line item description as it appears on the bill"
                        },
                        "category": {
                            "type": "string",
                            "enum": [
                                "base_charge", "energy_charge", "delivery_charge",
                                "fuel_cost_adjustment", "demand_charge", "rider_charge",
                                "tax", "fee", "credit", "other"
                            ],
                            "description": "Category of the charge"
                        },
                        "amount": {
                            "type": "number",
                            "description": "Dollar amount for this line item (negative for credits)"
                        },
                        "usage": {
                            "type": ["number", "null"],
                            "description": "Usage quantity for this line item if applicable"
                        },
                        "rate": {
                            "type": ["number", "null"],
                            "description": "Rate per unit for this line item if shown"
                        }
                    }
                }
            }
        }
    })
}

/// Build the completion request for bill extraction.
pub fn build_extraction_request(pdf_text: &str) -> CompletionRequest {
    let system = "\
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
- Extract the provider name exactly as it appears on the bill."
        .to_string();

    CompletionRequest {
        system,
        messages: vec![Message {
            role: Role::User,
            content: format!(
                "Extract the billing data from this utility bill:\n\n---\n{pdf_text}\n---"
            ),
        }],
        max_tokens: 2048,
        temperature: 0.0,
    }
}
