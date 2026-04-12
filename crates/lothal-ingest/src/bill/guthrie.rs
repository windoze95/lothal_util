use chrono::NaiveDate;
use regex::Regex;
use uuid::Uuid;

use lothal_core::{Bill, BillLineItem, LineItemCategory, Usd};

use crate::IngestError;

/// Parse a City of Guthrie water bill from extracted text.
pub fn parse_guthrie_bill(text: &str, account_id: Uuid) -> Result<Bill, IngestError> {
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
        "gallons".to_string(),
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

/// Parse billing period from Guthrie bill text.
fn parse_billing_period(text: &str) -> Result<(NaiveDate, NaiveDate), IngestError> {
    // "Service Period: MM/DD/YYYY - MM/DD/YYYY"
    let re1 = Regex::new(
        r"(?i)Service\s+(?:Period|From):?\s*(\d{1,2}/\d{1,2}/\d{4})\s*(?:To|[-–])\s*(\d{1,2}/\d{1,2}/\d{4})"
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

    // "Read Date: MM/DD/YYYY" paired with "Previous Read: MM/DD/YYYY"
    let re_prev = Regex::new(
        r"(?i)Previous\s+Read(?:\s+Date)?:?\s*(\d{1,2}/\d{1,2}/\d{4})"
    ).expect("valid regex");
    let re_curr = Regex::new(
        r"(?i)(?:Current\s+)?Read\s+Date:?\s*(\d{1,2}/\d{1,2}/\d{4})"
    ).expect("valid regex");

    if let (Some(prev_caps), Some(curr_caps)) = (re_prev.captures(text), re_curr.captures(text)) {
        let start = parse_mdy(&prev_caps[1])?;
        let end = parse_mdy(&curr_caps[1])?;
        return Ok((start, end));
    }

    // Fallback
    let re3 = Regex::new(
        r"(\d{1,2}/\d{1,2}/\d{4})\s*(?:to|[-–])\s*(\d{1,2}/\d{1,2}/\d{4})"
    ).expect("valid regex");

    if let Some(caps) = re3.captures(text) {
        let start = parse_mdy(&caps[1])?;
        let end = parse_mdy(&caps[2])?;
        return Ok((start, end));
    }

    Err(IngestError::Parse("could not find billing period in Guthrie bill".into()))
}

/// Parse statement / bill date.
fn parse_statement_date(text: &str) -> Result<NaiveDate, IngestError> {
    let re = Regex::new(
        r"(?i)(?:Statement|Bill|Invoice)\s+Date:?\s*(\d{1,2}/\d{1,2}/\d{4})"
    ).expect("valid regex");

    if let Some(caps) = re.captures(text) {
        return parse_mdy(&caps[1]);
    }

    // "Date: MM/DD/YYYY"
    let re2 = Regex::new(
        r"(?i)Date:?\s*(\d{1,2}/\d{1,2}/\d{4})"
    ).expect("valid regex");

    if let Some(caps) = re2.captures(text) {
        return parse_mdy(&caps[1]);
    }

    Err(IngestError::Parse("could not find statement date in Guthrie bill".into()))
}

/// Parse total water usage. Guthrie bills may report in gallons or thousand gallons.
fn parse_total_usage(text: &str) -> Result<f64, IngestError> {
    // Check thousand-gallon patterns FIRST to avoid the "Gallons" regex matching
    // "Thousand Gallons" and returning an unconverted value.

    // "Thousand Gallons Used: 4.5" or "Total Thousand Gallons: 4.5" or "kgal Used: 4.5"
    let re_kgal1 = Regex::new(
        r"(?i)(?:Total\s+)?(?:Thousand\s+Gallons|kgal)(?:\s+Used)?:?\s*([\d,]+(?:\.\d+)?)"
    ).expect("valid regex");

    if let Some(caps) = re_kgal1.captures(text) {
        let kgal = parse_number(&caps[1])?;
        return Ok(kgal * 1000.0);
    }

    // "Usage: XX kgal" or "Water: XX thousand gallons"
    let re_kgal2 = Regex::new(
        r"(?i)(?:Water\s+)?Usage:?\s*([\d,]+(?:\.\d+)?)\s*(?:thousand\s+gallons|kgal)"
    ).expect("valid regex");

    if let Some(caps) = re_kgal2.captures(text) {
        let kgal = parse_number(&caps[1])?;
        return Ok(kgal * 1000.0);
    }

    // "Total Gallons: X,XXX" or "Gallons Used: X,XXX"
    let re1 = Regex::new(
        r"(?i)(?:Total\s+)?Gallons(?:\s+Used)?:?\s*([\d,]+(?:\.\d+)?)"
    ).expect("valid regex");

    if let Some(caps) = re1.captures(text) {
        return parse_number(&caps[1]);
    }

    // "Usage: X,XXX gallons" or "Water Usage: X,XXX gal"
    let re2 = Regex::new(
        r"(?i)(?:Water\s+)?Usage:?\s*([\d,]+(?:\.\d+)?)\s*(?:gallons?|gal)"
    ).expect("valid regex");

    if let Some(caps) = re2.captures(text) {
        return parse_number(&caps[1]);
    }

    // "Consumption: X,XXX" — assume gallons
    let re5 = Regex::new(
        r"(?i)Consumption:?\s*([\d,]+(?:\.\d+)?)"
    ).expect("valid regex");

    if let Some(caps) = re5.captures(text) {
        return parse_number(&caps[1]);
    }

    Err(IngestError::Parse("could not find water usage in Guthrie bill".into()))
}

/// Parse total amount due.
fn parse_total_amount(text: &str) -> Result<Usd, IngestError> {
    let re1 = Regex::new(
        r"(?i)(?:Total\s+)?Amount\s+Due:?\s*\$\s*([\d,]+\.\d{2})"
    ).expect("valid regex");

    if let Some(caps) = re1.captures(text) {
        let val = parse_number(&caps[1])?;
        return Ok(Usd::new(val));
    }

    let re2 = Regex::new(
        r"(?i)Total\s+(?:Current\s+)?Charges:?\s*\$\s*([\d,]+\.\d{2})"
    ).expect("valid regex");

    if let Some(caps) = re2.captures(text) {
        let val = parse_number(&caps[1])?;
        return Ok(Usd::new(val));
    }

    let re3 = Regex::new(
        r"(?i)(?:Balance|Total)\s+Due:?\s*\$\s*([\d,]+\.\d{2})"
    ).expect("valid regex");

    if let Some(caps) = re3.captures(text) {
        let val = parse_number(&caps[1])?;
        return Ok(Usd::new(val));
    }

    Err(IngestError::Parse("could not find total amount in Guthrie bill".into()))
}

/// Parse line items from Guthrie water bill text.
fn parse_line_items(text: &str) -> Vec<(String, LineItemCategory, Usd)> {
    let mut items = Vec::new();

    let patterns: &[(&str, LineItemCategory)] = &[
        (r"(?i)(Water\s+(?:Service\s+)?Charge)\s+\$?\s*([\d,]+\.\d{2})", LineItemCategory::EnergyCharge),
        (r"(?i)(Water\s+Base\s+Charge)\s+\$?\s*([\d,]+\.\d{2})", LineItemCategory::BaseCharge),
        (r"(?i)(Water\s+Usage(?:\s+Charge)?)\s+\$?\s*([\d,]+\.\d{2})", LineItemCategory::EnergyCharge),
        (r"(?i)(Sewer\s+(?:Service\s+)?Charge)\s+\$?\s*([\d,]+\.\d{2})", LineItemCategory::Fee),
        (r"(?i)(Sewer(?:\s+Usage)?)\s+\$?\s*([\d,]+\.\d{2})", LineItemCategory::Fee),
        (r"(?i)(Trash\s+(?:Collection|Service|Charge))\s+\$?\s*([\d,]+\.\d{2})", LineItemCategory::Fee),
        (r"(?i)(Trash)\s+\$?\s*([\d,]+\.\d{2})", LineItemCategory::Fee),
        (r"(?i)(Storm\s*water(?:\s+(?:Fee|Charge))?)\s+\$?\s*([\d,]+\.\d{2})", LineItemCategory::Fee),
        (r"(?i)((?:State|City|County|Sales)\s+Tax)\s+\$?\s*([\d,]+\.\d{2})", LineItemCategory::Tax),
        (r"(?i)(Tax(?:es)?)\s+\$?\s*([\d,]+\.\d{2})", LineItemCategory::Tax),
        (r"(?i)(Late\s+(?:Fee|Charge|Penalty))\s+\$?\s*([\d,]+\.\d{2})", LineItemCategory::Fee),
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

    const SAMPLE_GUTHRIE: &str = "\
CITY OF GUTHRIE
Water and Sewer Department
Bill Date: 03/01/2026

Service Period: 01/28/2026 - 02/27/2026

Gallons Used: 4,500

Water Charge         $28.50
Sewer Charge         $22.00
Trash Collection     $18.75
City Tax             $3.46

Amount Due: $72.71
";

    #[test]
    fn test_parse_billing_period() {
        let (start, end) = parse_billing_period(SAMPLE_GUTHRIE).unwrap();
        assert_eq!(start, NaiveDate::from_ymd_opt(2026, 1, 28).unwrap());
        assert_eq!(end, NaiveDate::from_ymd_opt(2026, 2, 27).unwrap());
    }

    #[test]
    fn test_parse_statement_date() {
        let date = parse_statement_date(SAMPLE_GUTHRIE).unwrap();
        assert_eq!(date, NaiveDate::from_ymd_opt(2026, 3, 1).unwrap());
    }

    #[test]
    fn test_parse_total_usage() {
        let usage = parse_total_usage(SAMPLE_GUTHRIE).unwrap();
        assert!((usage - 4500.0).abs() < 0.01);
    }

    #[test]
    fn test_parse_total_amount() {
        let amount = parse_total_amount(SAMPLE_GUTHRIE).unwrap();
        assert!((amount.value() - 72.71).abs() < 0.01);
    }

    #[test]
    fn test_parse_line_items() {
        let items = parse_line_items(SAMPLE_GUTHRIE);
        assert!(items.len() >= 3);

        let water = items.iter().find(|(d, _, _)| d.contains("Water")).unwrap();
        assert_eq!(water.1, LineItemCategory::EnergyCharge);
        assert!((water.2.value() - 28.50).abs() < 0.01);

        let sewer = items.iter().find(|(d, _, _)| d.contains("Sewer")).unwrap();
        assert_eq!(sewer.1, LineItemCategory::Fee);

        let trash = items.iter().find(|(d, _, _)| d.contains("Trash")).unwrap();
        assert_eq!(trash.1, LineItemCategory::Fee);
    }

    #[test]
    fn test_parse_full_bill() {
        let account_id = Uuid::new_v4();
        let bill = parse_guthrie_bill(SAMPLE_GUTHRIE, account_id).unwrap();
        assert_eq!(bill.usage_unit, "gallons");
        assert!((bill.total_usage - 4500.0).abs() < 0.01);
        assert!((bill.total_amount.value() - 72.71).abs() < 0.01);
        assert!(!bill.line_items.is_empty());
    }

    #[test]
    fn test_kgal_conversion() {
        let text = "Thousand Gallons Used: 4.5";
        let usage = parse_total_usage(text).unwrap();
        assert!((usage - 4500.0).abs() < 0.01);
    }
}
