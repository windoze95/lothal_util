use chrono::NaiveDate;
use regex::Regex;
use uuid::Uuid;

use lothal_core::{Bill, BillLineItem, LineItemCategory, Usd};

use crate::IngestError;

/// Parse an ONG (Oklahoma Natural Gas) bill from extracted text.
pub fn parse_ong_bill(text: &str, account_id: Uuid) -> Result<Bill, IngestError> {
    let (period_start, period_end) = parse_billing_period(text)?;
    let statement_date = parse_statement_date(text)?;
    let total_usage = parse_total_usage(text)?;
    let total_amount = parse_total_amount(text)?;
    let line_items_raw = parse_line_items(text);

    let mut bill = Bill::new(
        account_id,
        period_start,
        period_end,
        statement_date,
        total_usage,
        "therms".to_string(),
        total_amount,
    );

    for (description, category, amount) in &line_items_raw {
        let li = BillLineItem::new(
            bill.id,
            description.clone(),
            *category,
            *amount,
        );
        bill.line_items.push(li);
    }

    Ok(bill)
}

/// Parse billing period from ONG bill text.
fn parse_billing_period(text: &str) -> Result<(NaiveDate, NaiveDate), IngestError> {
    // "Service From: MM/DD/YYYY To: MM/DD/YYYY"
    let re1 = Regex::new(
        r"(?i)Service\s+(?:From|Period):?\s*(\d{1,2}/\d{1,2}/\d{4})\s+(?:To:?\s+|[-–]\s*)(\d{1,2}/\d{1,2}/\d{4})"
    ).expect("valid regex");

    if let Some(caps) = re1.captures(text) {
        let start = parse_mdy(&caps[1])?;
        let end = parse_mdy(&caps[2])?;
        return Ok((start, end));
    }

    // "Billing Period: MMM DD, YYYY - MMM DD, YYYY"
    let re2 = Regex::new(
        r"(?i)Billing\s+Period:?\s*([A-Za-z]{3}\s+\d{1,2},?\s*\d{4})\s*[-–]\s*([A-Za-z]{3}\s+\d{1,2},?\s*\d{4})"
    ).expect("valid regex");

    if let Some(caps) = re2.captures(text) {
        let start = parse_month_day_year(&caps[1])?;
        let end = parse_month_day_year(&caps[2])?;
        return Ok((start, end));
    }

    // Fallback: any date range
    let re3 = Regex::new(
        r"(\d{1,2}/\d{1,2}/\d{4})\s*(?:to|[-–])\s*(\d{1,2}/\d{1,2}/\d{4})"
    ).expect("valid regex");

    if let Some(caps) = re3.captures(text) {
        let start = parse_mdy(&caps[1])?;
        let end = parse_mdy(&caps[2])?;
        return Ok((start, end));
    }

    Err(IngestError::Parse("could not find billing period in ONG bill".into()))
}

/// Parse statement date from ONG bill text.
fn parse_statement_date(text: &str) -> Result<NaiveDate, IngestError> {
    let re = Regex::new(
        r"(?i)(?:Statement|Bill|Invoice)\s+Date:?\s*(\d{1,2}/\d{1,2}/\d{4})"
    ).expect("valid regex");

    if let Some(caps) = re.captures(text) {
        return parse_mdy(&caps[1]);
    }

    // "Date: MMM DD, YYYY"
    let re2 = Regex::new(
        r"(?i)Date:?\s*([A-Za-z]{3}\s+\d{1,2},?\s*\d{4})"
    ).expect("valid regex");

    if let Some(caps) = re2.captures(text) {
        return parse_month_day_year(&caps[1]);
    }

    Err(IngestError::Parse("could not find statement date in ONG bill".into()))
}

/// Parse total gas usage in therms or CCF.
fn parse_total_usage(text: &str) -> Result<f64, IngestError> {
    // "Total Therms: X,XXX" or "Therms Used: X,XXX"
    let re1 = Regex::new(
        r"(?i)(?:Total\s+)?Therms(?:\s+Used)?:?\s*([\d,]+(?:\.\d+)?)"
    ).expect("valid regex");

    if let Some(caps) = re1.captures(text) {
        return parse_number(&caps[1]);
    }

    // "Usage: XX.X therms"
    let re2 = Regex::new(
        r"(?i)Usage:?\s*([\d,]+(?:\.\d+)?)\s*therms"
    ).expect("valid regex");

    if let Some(caps) = re2.captures(text) {
        return parse_number(&caps[1]);
    }

    // CCF (100 cubic feet) - convert to therms (1 CCF ~ 1.037 therms)
    let re3 = Regex::new(
        r"(?i)(?:Total\s+)?CCF(?:\s+Used)?:?\s*([\d,]+(?:\.\d+)?)"
    ).expect("valid regex");

    if let Some(caps) = re3.captures(text) {
        let ccf = parse_number(&caps[1])?;
        return Ok(ccf * 1.037);
    }

    // "Usage: XX.X CCF"
    let re4 = Regex::new(
        r"(?i)Usage:?\s*([\d,]+(?:\.\d+)?)\s*CCF"
    ).expect("valid regex");

    if let Some(caps) = re4.captures(text) {
        let ccf = parse_number(&caps[1])?;
        return Ok(ccf * 1.037);
    }

    Err(IngestError::Parse("could not find total usage (therms or CCF) in ONG bill".into()))
}

/// Parse total amount due from ONG bill text.
fn parse_total_amount(text: &str) -> Result<Usd, IngestError> {
    // "Amount Due: $XXX.XX"
    let re1 = Regex::new(
        r"(?i)Amount\s+Due:?\s*\$\s*([\d,]+\.\d{2})"
    ).expect("valid regex");

    if let Some(caps) = re1.captures(text) {
        let val = parse_number(&caps[1])?;
        return Ok(Usd::new(val));
    }

    // "Total Current Charges: $XXX.XX"
    let re2 = Regex::new(
        r"(?i)Total\s+(?:Current\s+)?Charges:?\s*\$\s*([\d,]+\.\d{2})"
    ).expect("valid regex");

    if let Some(caps) = re2.captures(text) {
        let val = parse_number(&caps[1])?;
        return Ok(Usd::new(val));
    }

    // "Total Due: $XXX.XX"
    let re3 = Regex::new(
        r"(?i)Total\s+Due:?\s*\$\s*([\d,]+\.\d{2})"
    ).expect("valid regex");

    if let Some(caps) = re3.captures(text) {
        let val = parse_number(&caps[1])?;
        return Ok(Usd::new(val));
    }

    Err(IngestError::Parse("could not find total amount in ONG bill".into()))
}

/// Parse line items from ONG bill text.
fn parse_line_items(text: &str) -> Vec<(String, LineItemCategory, Usd)> {
    let mut items = Vec::new();

    let patterns: &[(&str, LineItemCategory)] = &[
        (r"(?i)(Customer\s+Charge)\s+\$?\s*([\d,]+\.\d{2})", LineItemCategory::BaseCharge),
        (r"(?i)(Base\s+(?:Service\s+)?Charge)\s+\$?\s*([\d,]+\.\d{2})", LineItemCategory::BaseCharge),
        (r"(?i)(Gas\s+Cost(?:\s+Recovery)?)\s+\$?\s*([\d,]+\.\d{2})", LineItemCategory::EnergyCharge),
        (r"(?i)(Natural\s+Gas\s+(?:Cost|Charge))\s+\$?\s*([\d,]+\.\d{2})", LineItemCategory::EnergyCharge),
        (r"(?i)(Commodity\s+Charge)\s+\$?\s*([\d,]+\.\d{2})", LineItemCategory::EnergyCharge),
        (r"(?i)(Delivery\s+Charge)\s+\$?\s*([\d,]+\.\d{2})", LineItemCategory::DeliveryCharge),
        (r"(?i)(Distribution\s+Charge)\s+\$?\s*([\d,]+\.\d{2})", LineItemCategory::DeliveryCharge),
        (r"(?i)(Transportation\s+Charge)\s+\$?\s*([\d,]+\.\d{2})", LineItemCategory::DeliveryCharge),
        (r"(?i)((?:State|City|County|Sales|Franchise|Gross\s+Receipts)\s+Tax)\s+\$?\s*([\d,]+\.\d{2})", LineItemCategory::Tax),
        (r"(?i)(Tax(?:es)?)\s+\$?\s*([\d,]+\.\d{2})", LineItemCategory::Tax),
        (r"(?i)(Regulatory\s+(?:Assessment|Fee|Charge))\s+\$?\s*([\d,]+\.\d{2})", LineItemCategory::Fee),
        (r"(?i)(Late\s+(?:Fee|Charge|Payment\s+Charge))\s+\$?\s*([\d,]+\.\d{2})", LineItemCategory::Fee),
        (r"(?i)(Credit)\s+\$?\s*(-?[\d,]+\.\d{2})", LineItemCategory::Credit),
    ];

    for (pattern, category) in patterns {
        let re = Regex::new(pattern).expect("valid regex");
        for caps in re.captures_iter(text) {
            let description = caps[1].trim().to_string();
            if let Ok(val) = parse_number(&caps[2]) {
                items.push((description, *category, Usd::new(val)));
            }
        }
    }

    items
}

fn parse_mdy(s: &str) -> Result<NaiveDate, IngestError> {
    NaiveDate::parse_from_str(s.trim(), "%-m/%-d/%Y")
        .map_err(|e| IngestError::Parse(format!("invalid date '{s}': {e}")))
}

fn parse_month_day_year(s: &str) -> Result<NaiveDate, IngestError> {
    let normalized = s.trim().replace(',', "");
    let normalized = Regex::new(r"\s+").expect("valid regex").replace_all(&normalized, " ");
    NaiveDate::parse_from_str(&normalized, "%b %-d %Y")
        .map_err(|e| IngestError::Parse(format!("invalid date '{s}': {e}")))
}

fn parse_number(s: &str) -> Result<f64, IngestError> {
    let cleaned = s.replace(',', "");
    cleaned.parse::<f64>()
        .map_err(|e| IngestError::Parse(format!("invalid number '{s}': {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_ONG: &str = "\
OKLAHOMA NATURAL GAS
Statement Date: 02/20/2026

Service From: 01/18/2026 To: 02/17/2026

Therms Used: 85.3

Customer Charge      $12.50
Gas Cost             $48.22
Delivery Charge      $18.75
State Tax            $3.97
City Tax             $2.41

Amount Due: $85.85
";

    #[test]
    fn test_parse_billing_period() {
        let (start, end) = parse_billing_period(SAMPLE_ONG).unwrap();
        assert_eq!(start, NaiveDate::from_ymd_opt(2026, 1, 18).unwrap());
        assert_eq!(end, NaiveDate::from_ymd_opt(2026, 2, 17).unwrap());
    }

    #[test]
    fn test_parse_statement_date() {
        let date = parse_statement_date(SAMPLE_ONG).unwrap();
        assert_eq!(date, NaiveDate::from_ymd_opt(2026, 2, 20).unwrap());
    }

    #[test]
    fn test_parse_total_usage() {
        let usage = parse_total_usage(SAMPLE_ONG).unwrap();
        assert!((usage - 85.3).abs() < 0.01);
    }

    #[test]
    fn test_parse_total_amount() {
        let amount = parse_total_amount(SAMPLE_ONG).unwrap();
        assert!((amount.value() - 85.85).abs() < 0.01);
    }

    #[test]
    fn test_parse_line_items() {
        let items = parse_line_items(SAMPLE_ONG);
        assert!(items.len() >= 4);

        let customer = items.iter().find(|(d, _, _)| d.contains("Customer")).unwrap();
        assert_eq!(customer.1, LineItemCategory::BaseCharge);

        let gas = items.iter().find(|(d, _, _)| d.contains("Gas Cost")).unwrap();
        assert_eq!(gas.1, LineItemCategory::EnergyCharge);

        let delivery = items.iter().find(|(d, _, _)| d.contains("Delivery")).unwrap();
        assert_eq!(delivery.1, LineItemCategory::DeliveryCharge);
    }

    #[test]
    fn test_parse_full_bill() {
        let account_id = Uuid::new_v4();
        let bill = parse_ong_bill(SAMPLE_ONG, account_id).unwrap();
        assert_eq!(bill.usage_unit, "therms");
        assert!((bill.total_usage - 85.3).abs() < 0.01);
        assert!((bill.total_amount.value() - 85.85).abs() < 0.01);
    }

    #[test]
    fn test_ccf_to_therms() {
        let text = "\
OKLAHOMA NATURAL GAS
Statement Date: 01/15/2026
Billing Period: Dec 15, 2025 - Jan 14, 2026
CCF Used: 50
Amount Due: $60.00
";
        let usage = parse_total_usage(text).unwrap();
        assert!((usage - 51.85).abs() < 0.1); // 50 * 1.037
    }
}
