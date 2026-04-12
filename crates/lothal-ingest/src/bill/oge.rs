use chrono::NaiveDate;
use regex::Regex;
use uuid::Uuid;

use lothal_core::{Bill, BillLineItem, LineItemCategory, Usd};

use crate::IngestError;

/// Parse an OG&E (Oklahoma Gas & Electric) bill from extracted text.
pub fn parse_oge_bill(text: &str, account_id: Uuid) -> Result<Bill, IngestError> {
    let (period_start, period_end) = parse_billing_period(text)?;
    let statement_date = parse_statement_date(text)?;
    let total_kwh = parse_total_kwh(text)?;
    let total_amount = parse_total_amount(text)?;
    let line_items_raw = parse_line_items(text);

    let mut bill = Bill::new(
        account_id,
        period_start,
        period_end,
        statement_date,
        total_kwh,
        "kWh".to_string(),
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

/// Parse billing period from text, trying several formats.
fn parse_billing_period(text: &str) -> Result<(NaiveDate, NaiveDate), IngestError> {
    // "Service From: MM/DD/YYYY To: MM/DD/YYYY"
    let re1 = Regex::new(
        r"(?i)Service\s+From:?\s*(\d{1,2}/\d{1,2}/\d{4})\s+To:?\s*(\d{1,2}/\d{1,2}/\d{4})"
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

    // "Service Period MM/DD/YYYY - MM/DD/YYYY"
    let re3 = Regex::new(
        r"(?i)Service\s+Period:?\s*(\d{1,2}/\d{1,2}/\d{4})\s*[-–]\s*(\d{1,2}/\d{1,2}/\d{4})"
    ).expect("valid regex");

    if let Some(caps) = re3.captures(text) {
        let start = parse_mdy(&caps[1])?;
        let end = parse_mdy(&caps[2])?;
        return Ok((start, end));
    }

    // "MM/DD/YYYY to MM/DD/YYYY" or "MM/DD/YYYY - MM/DD/YYYY" standalone
    let re4 = Regex::new(
        r"(\d{1,2}/\d{1,2}/\d{4})\s*(?:to|[-–])\s*(\d{1,2}/\d{1,2}/\d{4})"
    ).expect("valid regex");

    if let Some(caps) = re4.captures(text) {
        let start = parse_mdy(&caps[1])?;
        let end = parse_mdy(&caps[2])?;
        return Ok((start, end));
    }

    Err(IngestError::Parse("could not find billing period in OG&E bill".into()))
}

/// Parse statement date from text.
fn parse_statement_date(text: &str) -> Result<NaiveDate, IngestError> {
    let re = Regex::new(
        r"(?i)Statement\s+Date:?\s*(\d{1,2}/\d{1,2}/\d{4})"
    ).expect("valid regex");

    if let Some(caps) = re.captures(text) {
        return parse_mdy(&caps[1]);
    }

    // Try "Date: MM/DD/YYYY"
    let re2 = Regex::new(
        r"(?i)(?:Bill|Invoice)\s+Date:?\s*(\d{1,2}/\d{1,2}/\d{4})"
    ).expect("valid regex");

    if let Some(caps) = re2.captures(text) {
        return parse_mdy(&caps[1]);
    }

    Err(IngestError::Parse("could not find statement date in OG&E bill".into()))
}

/// Parse total kWh from text.
fn parse_total_kwh(text: &str) -> Result<f64, IngestError> {
    // "Total kWh Used: X,XXX" or "Total kWh: X,XXX"
    let re1 = Regex::new(
        r"(?i)Total\s+kWh(?:\s+Used)?:?\s*([\d,]+(?:\.\d+)?)"
    ).expect("valid regex");

    if let Some(caps) = re1.captures(text) {
        return parse_number(&caps[1]);
    }

    // "Usage: X,XXX kWh"
    let re2 = Regex::new(
        r"(?i)Usage:?\s*([\d,]+(?:\.\d+)?)\s*kWh"
    ).expect("valid regex");

    if let Some(caps) = re2.captures(text) {
        return parse_number(&caps[1]);
    }

    // "X,XXX kWh" near "used" or "consumption"
    let re3 = Regex::new(
        r"(?i)(?:used|consumption)\s*:?\s*([\d,]+(?:\.\d+)?)\s*kWh"
    ).expect("valid regex");

    if let Some(caps) = re3.captures(text) {
        return parse_number(&caps[1]);
    }

    Err(IngestError::Parse("could not find total kWh in OG&E bill".into()))
}

/// Parse total amount due.
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
        r"(?i)Total\s+Current\s+Charges:?\s*\$\s*([\d,]+\.\d{2})"
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

    Err(IngestError::Parse("could not find total amount in OG&E bill".into()))
}

/// Parse line items from the bill text.
fn parse_line_items(text: &str) -> Vec<(String, LineItemCategory, Usd)> {
    let mut items = Vec::new();

    let patterns: &[(&str, LineItemCategory)] = &[
        (r"(?i)(Customer\s+Charge)\s+\$?\s*([\d,]+\.\d{2})", LineItemCategory::BaseCharge),
        (r"(?i)(Base\s+(?:Service\s+)?Charge)\s+\$?\s*([\d,]+\.\d{2})", LineItemCategory::BaseCharge),
        (r"(?i)(Energy\s+Charge)\s+\$?\s*([\d,]+\.\d{2})", LineItemCategory::EnergyCharge),
        (r"(?i)(Fuel\s+Cost\s+Adj(?:ustment)?)\s+\$?\s*(-?[\d,]+\.\d{2})", LineItemCategory::FuelCostAdjustment),
        (r"(?i)(Fuel\s+Charge)\s+\$?\s*(-?[\d,]+\.\d{2})", LineItemCategory::FuelCostAdjustment),
        (r"(?i)(Demand\s+Charge)\s+\$?\s*([\d,]+\.\d{2})", LineItemCategory::DemandCharge),
        (r"(?i)(Rider\s+[^\$\n]+?)\s+\$?\s*(-?[\d,]+\.\d{2})", LineItemCategory::RiderCharge),
        (r"(?i)((?:State|City|County|Sales|Franchise)\s+Tax)\s+\$?\s*([\d,]+\.\d{2})", LineItemCategory::Tax),
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

/// Parse "MM/DD/YYYY" into a NaiveDate.
fn parse_mdy(s: &str) -> Result<NaiveDate, IngestError> {
    NaiveDate::parse_from_str(s.trim(), "%-m/%-d/%Y")
        .map_err(|e| IngestError::Parse(format!("invalid date '{s}': {e}")))
}

/// Parse "MMM DD, YYYY" or "MMM DD YYYY" into a NaiveDate.
fn parse_month_day_year(s: &str) -> Result<NaiveDate, IngestError> {
    let normalized = s.trim().replace(',', "");
    // Collapse multiple spaces
    let normalized = Regex::new(r"\s+").expect("valid regex").replace_all(&normalized, " ");
    NaiveDate::parse_from_str(&normalized, "%b %-d %Y")
        .map_err(|e| IngestError::Parse(format!("invalid date '{s}': {e}")))
}

/// Parse a number string that may contain commas and an optional leading minus sign.
fn parse_number(s: &str) -> Result<f64, IngestError> {
    let cleaned = s.replace(',', "");
    cleaned.parse::<f64>()
        .map_err(|e| IngestError::Parse(format!("invalid number '{s}': {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_OGE: &str = "\
OG&E
OKLAHOMA GAS AND ELECTRIC COMPANY
Statement Date: 03/15/2026

Service From: 02/13/2026 To: 03/13/2026

Account Number: 1234567890

Total kWh Used: 1,245

Customer Charge      $10.00
Energy Charge        $95.42
Fuel Cost Adjustment $12.38
Rider Wind           $3.50
State Tax            $5.25
City Tax             $3.18

Amount Due: $129.73
";

    #[test]
    fn test_parse_billing_period() {
        let (start, end) = parse_billing_period(SAMPLE_OGE).unwrap();
        assert_eq!(start, NaiveDate::from_ymd_opt(2026, 2, 13).unwrap());
        assert_eq!(end, NaiveDate::from_ymd_opt(2026, 3, 13).unwrap());
    }

    #[test]
    fn test_parse_statement_date() {
        let date = parse_statement_date(SAMPLE_OGE).unwrap();
        assert_eq!(date, NaiveDate::from_ymd_opt(2026, 3, 15).unwrap());
    }

    #[test]
    fn test_parse_total_kwh() {
        let kwh = parse_total_kwh(SAMPLE_OGE).unwrap();
        assert!((kwh - 1245.0).abs() < 0.01);
    }

    #[test]
    fn test_parse_total_amount() {
        let amount = parse_total_amount(SAMPLE_OGE).unwrap();
        assert!((amount.value() - 129.73).abs() < 0.01);
    }

    #[test]
    fn test_parse_line_items() {
        let items = parse_line_items(SAMPLE_OGE);
        assert!(items.len() >= 5);

        let customer = items.iter().find(|(d, _, _)| d.contains("Customer")).unwrap();
        assert_eq!(customer.1, LineItemCategory::BaseCharge);
        assert!((customer.2.value() - 10.0).abs() < 0.01);

        let energy = items.iter().find(|(d, _, _)| d.contains("Energy")).unwrap();
        assert_eq!(energy.1, LineItemCategory::EnergyCharge);
        assert!((energy.2.value() - 95.42).abs() < 0.01);
    }

    #[test]
    fn test_parse_full_bill() {
        let account_id = Uuid::new_v4();
        let bill = parse_oge_bill(SAMPLE_OGE, account_id).unwrap();
        assert_eq!(bill.account_id, account_id);
        assert_eq!(bill.usage_unit, "kWh");
        assert!((bill.total_usage - 1245.0).abs() < 0.01);
        assert!((bill.total_amount.value() - 129.73).abs() < 0.01);
        assert!(!bill.line_items.is_empty());
    }

    #[test]
    fn test_parse_billing_period_alt_format() {
        let text = "Billing Period: Mar 01, 2026 - Mar 31, 2026";
        let (start, end) = parse_billing_period(text).unwrap();
        assert_eq!(start, NaiveDate::from_ymd_opt(2026, 3, 1).unwrap());
        assert_eq!(end, NaiveDate::from_ymd_opt(2026, 3, 31).unwrap());
    }

    #[test]
    fn test_parse_mdy() {
        let d = parse_mdy("3/5/2026").unwrap();
        assert_eq!(d, NaiveDate::from_ymd_opt(2026, 3, 5).unwrap());
    }

    #[test]
    fn test_parse_number_with_commas() {
        assert!((parse_number("1,245").unwrap() - 1245.0).abs() < 0.01);
        assert!((parse_number("12,345.67").unwrap() - 12345.67).abs() < 0.01);
        assert!((parse_number("-3.50").unwrap() - (-3.50)).abs() < 0.01);
    }
}
