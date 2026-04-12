use chrono::NaiveDate;
use uuid::Uuid;

use lothal_core::ontology::bill::{Bill, BillLineItem, LineItemCategory, LineItemValidation};
use lothal_core::units::Usd;

use super::schema::{self, ExtractedBill, ExtractedLineItem};
use crate::provider::{CompletionRequest, LlmClient, Message, Role};
use crate::AiError;

const MAX_RETRIES: u32 = 2;

/// Convert an `ExtractedBill` to a `Bill`, validate, and retry with correction
/// prompt if line items don't sum correctly.
pub async fn validate_and_convert(
    extracted: ExtractedBill,
    account_id: Uuid,
    provider: &LlmClient,
    original_text: &str,
) -> Result<Bill, AiError> {
    let mut bill = convert_extracted_to_bill(&extracted, account_id)?;

    match bill.validate_line_items() {
        LineItemValidation::Valid => return Ok(bill),
        LineItemValidation::Mismatch {
            expected,
            actual,
            difference,
        } => {
            tracing::warn!(
                "Line items mismatch: expected {expected}, got {actual} (diff {difference}). Retrying..."
            );
        }
    }

    // Retry loop with correction prompt.
    for attempt in 1..=MAX_RETRIES {
        tracing::info!("Retry {attempt}/{MAX_RETRIES} for line item validation");

        let correction_request = build_correction_request(original_text, &extracted);
        let json_schema = schema::bill_json_schema();
        let raw = provider
            .complete_json(&correction_request, &json_schema)
            .await?;
        let retried: ExtractedBill = serde_json::from_value(raw)?;
        bill = convert_extracted_to_bill(&retried, account_id)?;

        if let LineItemValidation::Valid = bill.validate_line_items() {
            return Ok(bill);
        }
    }

    Err(AiError::ValidationExhausted {
        attempts: MAX_RETRIES + 1,
        message: "Line items do not sum to total amount".into(),
    })
}

fn convert_extracted_to_bill(
    extracted: &ExtractedBill,
    account_id: Uuid,
) -> Result<Bill, AiError> {
    let period_start = parse_date(&extracted.period_start, "period_start")?;
    let period_end = parse_date(&extracted.period_end, "period_end")?;
    let statement_date = parse_date(&extracted.statement_date, "statement_date")?;

    let mut bill = Bill::new(
        account_id,
        period_start,
        period_end,
        statement_date,
        extracted.total_usage,
        extracted.usage_unit.clone(),
        Usd::new(extracted.total_amount),
    );

    bill.line_items = extracted
        .line_items
        .iter()
        .map(|li| convert_line_item(li, bill.id))
        .collect();

    Ok(bill)
}

fn convert_line_item(item: &ExtractedLineItem, bill_id: Uuid) -> BillLineItem {
    let mut li = BillLineItem::new(
        bill_id,
        item.description.clone(),
        parse_category(&item.category),
        Usd::new(item.amount),
    );
    li.usage = item.usage;
    li.rate = item.rate;
    li
}

fn parse_date(s: &str, field: &str) -> Result<NaiveDate, AiError> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .map_err(|e| AiError::Validation(format!("Invalid date for {field}: '{s}' ({e})")))
}

fn parse_category(s: &str) -> LineItemCategory {
    match s.to_lowercase().replace(' ', "_").as_str() {
        "base_charge" => LineItemCategory::BaseCharge,
        "energy_charge" => LineItemCategory::EnergyCharge,
        "delivery_charge" => LineItemCategory::DeliveryCharge,
        "fuel_cost_adjustment" => LineItemCategory::FuelCostAdjustment,
        "demand_charge" => LineItemCategory::DemandCharge,
        "rider_charge" => LineItemCategory::RiderCharge,
        "tax" => LineItemCategory::Tax,
        "fee" => LineItemCategory::Fee,
        "credit" => LineItemCategory::Credit,
        _ => LineItemCategory::Other,
    }
}

fn build_correction_request(
    original_text: &str,
    previous: &ExtractedBill,
) -> CompletionRequest {
    let line_total: f64 = previous.line_items.iter().map(|li| li.amount).sum();

    let system = "\
You are a utility bill data extraction assistant. Your previous extraction had \
an error: the line item amounts did not sum to the total. Please re-examine the \
bill text carefully and produce corrected output.

Rules:
- All dates must be in YYYY-MM-DD format.
- All dollar amounts must be numeric (no $ signs).
- The line_items amounts MUST sum to total_amount (within $0.02).
- Double-check each amount against the original text."
        .to_string();

    CompletionRequest {
        system,
        messages: vec![Message {
            role: Role::User,
            content: format!(
                "Your previous extraction had line items summing to ${line_total:.2} \
                 but total_amount was ${:.2}. Please re-extract from the original bill:\n\n\
                 ---\n{original_text}\n---",
                previous.total_amount
            ),
        }],
        max_tokens: 2048,
        temperature: 0.0,
        budget_tokens: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_category() {
        assert_eq!(parse_category("base_charge"), LineItemCategory::BaseCharge);
        assert_eq!(parse_category("energy_charge"), LineItemCategory::EnergyCharge);
        assert_eq!(parse_category("tax"), LineItemCategory::Tax);
        assert_eq!(parse_category("unknown_thing"), LineItemCategory::Other);
    }

    #[test]
    fn test_convert_extracted_valid() {
        let extracted = ExtractedBill {
            provider_name: "OG&E".into(),
            period_start: "2026-01-01".into(),
            period_end: "2026-01-31".into(),
            statement_date: "2026-02-01".into(),
            total_usage: 1200.0,
            usage_unit: "kWh".into(),
            total_amount: 150.00,
            line_items: vec![
                ExtractedLineItem {
                    description: "Base charge".into(),
                    category: "base_charge".into(),
                    amount: 15.00,
                    usage: None,
                    rate: None,
                },
                ExtractedLineItem {
                    description: "Energy".into(),
                    category: "energy_charge".into(),
                    amount: 120.00,
                    usage: Some(1200.0),
                    rate: Some(0.10),
                },
                ExtractedLineItem {
                    description: "Tax".into(),
                    category: "tax".into(),
                    amount: 15.00,
                    usage: None,
                    rate: None,
                },
            ],
        };

        let bill =
            convert_extracted_to_bill(&extracted, Uuid::new_v4()).unwrap();
        assert_eq!(bill.total_usage, 1200.0);
        assert_eq!(bill.total_amount.value(), 150.00);
        assert_eq!(bill.line_items.len(), 3);
        assert!(matches!(
            bill.validate_line_items(),
            LineItemValidation::Valid
        ));
    }

    #[test]
    fn test_convert_extracted_mismatch() {
        let extracted = ExtractedBill {
            provider_name: "OG&E".into(),
            period_start: "2026-01-01".into(),
            period_end: "2026-01-31".into(),
            statement_date: "2026-02-01".into(),
            total_usage: 1200.0,
            usage_unit: "kWh".into(),
            total_amount: 150.00,
            line_items: vec![ExtractedLineItem {
                description: "Energy".into(),
                category: "energy_charge".into(),
                amount: 120.00,
                usage: None,
                rate: None,
            }],
        };

        let bill =
            convert_extracted_to_bill(&extracted, Uuid::new_v4()).unwrap();
        assert!(matches!(
            bill.validate_line_items(),
            LineItemValidation::Mismatch { .. }
        ));
    }

    #[test]
    fn test_parse_date_valid() {
        let d = parse_date("2026-03-15", "test").unwrap();
        assert_eq!(d.to_string(), "2026-03-15");
    }

    #[test]
    fn test_parse_date_invalid() {
        assert!(parse_date("March 15", "test").is_err());
    }
}
