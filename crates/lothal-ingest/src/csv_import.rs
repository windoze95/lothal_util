use std::path::Path;

use chrono::{DateTime, NaiveDate, NaiveDateTime, TimeZone, Utc};
use serde::Deserialize;
use uuid::Uuid;

use lothal_core::{Bill, Reading, ReadingKind, ReadingSource, Usd};

use crate::IngestError;

// ---------------------------------------------------------------------------
// OG&E billing-period CSV
// ---------------------------------------------------------------------------

/// A row from an OG&E portal CSV export (billing summary).
#[derive(Debug, Deserialize)]
struct OgeBillingRow {
    #[serde(alias = "Date", alias = "date", alias = "Billing Date")]
    date: String,
    #[serde(alias = "Usage (kWh)", alias = "Usage", alias = "kWh", alias = "usage_kwh")]
    usage: String,
    #[serde(alias = "Cost ($)", alias = "Cost", alias = "Amount", alias = "cost")]
    cost: String,
    #[serde(alias = "Type", alias = "type", default)]
    row_type: Option<String>,
}

/// Parse an OG&E billing-summary CSV into `Bill` objects (one per row).
///
/// Expected columns: Date, Usage (kWh), Cost ($), optional Type.
/// The "Date" column is treated as the statement date; because the CSV
/// typically does not carry explicit period start/end fields, we approximate
/// by placing the period end at the statement date and start 30 days prior.
pub fn parse_oge_csv(path: &Path, account_id: Uuid) -> Result<Vec<Bill>, IngestError> {
    let mut reader = csv::ReaderBuilder::new()
        .flexible(true)
        .trim(csv::Trim::All)
        .from_path(path)?;

    let mut bills: Vec<Bill> = Vec::new();

    for result in reader.deserialize() {
        let row: OgeBillingRow = result?;

        let statement_date = parse_csv_date(&row.date)?;
        let usage = parse_csv_number(&row.usage)?;
        let cost = parse_csv_number(&row.cost)?;

        // Approximate billing period: 30 days ending on the statement date.
        let period_end = statement_date;
        let period_start = period_end - chrono::Duration::days(30);

        let mut bill = Bill::new(
            account_id,
            period_start,
            period_end,
            statement_date,
            usage,
            "kWh".to_string(),
            Usd::new(cost),
        );

        bill.source_file = Some(path.display().to_string());
        if let Some(ref t) = row.row_type {
            if !t.is_empty() {
                bill.notes = Some(format!("Row type: {t}"));
            }
        }

        bills.push(bill);
    }

    if bills.is_empty() {
        return Err(IngestError::Parse("OG&E CSV contained no data rows".into()));
    }

    Ok(bills)
}

// ---------------------------------------------------------------------------
// OG&E hourly / daily usage CSV
// ---------------------------------------------------------------------------

/// A row from an OG&E hourly or daily usage export.
#[derive(Debug, Deserialize)]
struct OgeUsageRow {
    #[serde(alias = "Date", alias = "date", alias = "DateTime", alias = "Timestamp")]
    date: String,
    #[serde(alias = "Usage (kWh)", alias = "Usage", alias = "kWh", alias = "usage_kwh", alias = "Value")]
    usage: String,
}

/// Parse an OG&E hourly / daily usage CSV into `Reading` objects.
///
/// Each row becomes a single `Reading` with `ReadingKind::ElectricKwh`.
pub fn parse_oge_usage_csv(
    path: &Path,
    source: ReadingSource,
) -> Result<Vec<Reading>, IngestError> {
    let mut reader = csv::ReaderBuilder::new()
        .flexible(true)
        .trim(csv::Trim::All)
        .from_path(path)?;

    let mut readings: Vec<Reading> = Vec::new();

    for result in reader.deserialize() {
        let row: OgeUsageRow = result?;

        let time = parse_csv_datetime(&row.date)?;
        let kwh = parse_csv_number(&row.usage)?;

        readings.push(Reading::at(time, source, ReadingKind::ElectricKwh, kwh));
    }

    if readings.is_empty() {
        return Err(IngestError::Parse("OG&E usage CSV contained no data rows".into()));
    }

    Ok(readings)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Try several date formats commonly seen in OG&E CSVs.
fn parse_csv_date(s: &str) -> Result<NaiveDate, IngestError> {
    let s = s.trim();

    // MM/DD/YYYY
    if let Ok(d) = NaiveDate::parse_from_str(s, "%-m/%-d/%Y") {
        return Ok(d);
    }

    // YYYY-MM-DD
    if let Ok(d) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return Ok(d);
    }

    // MM-DD-YYYY
    if let Ok(d) = NaiveDate::parse_from_str(s, "%-m-%-d-%Y") {
        return Ok(d);
    }

    // "MMM DD, YYYY"
    let normalized = s.replace(',', "");
    if let Ok(d) = NaiveDate::parse_from_str(normalized.trim(), "%b %-d %Y") {
        return Ok(d);
    }

    Err(IngestError::Parse(format!("could not parse CSV date '{s}'")))
}

/// Try several datetime formats for hourly/daily usage rows.
fn parse_csv_datetime(s: &str) -> Result<DateTime<Utc>, IngestError> {
    let s = s.trim();

    // ISO-8601 with T separator
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Ok(dt.with_timezone(&Utc));
    }

    // "YYYY-MM-DD HH:MM:SS"
    if let Ok(ndt) = NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S") {
        return Ok(Utc.from_utc_datetime(&ndt));
    }

    // "YYYY-MM-DD HH:MM"
    if let Ok(ndt) = NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M") {
        return Ok(Utc.from_utc_datetime(&ndt));
    }

    // "MM/DD/YYYY HH:MM:SS"
    if let Ok(ndt) = NaiveDateTime::parse_from_str(s, "%-m/%-d/%Y %H:%M:%S") {
        return Ok(Utc.from_utc_datetime(&ndt));
    }

    // "MM/DD/YYYY HH:MM"
    if let Ok(ndt) = NaiveDateTime::parse_from_str(s, "%-m/%-d/%Y %H:%M") {
        return Ok(Utc.from_utc_datetime(&ndt));
    }

    // Date-only -> midnight UTC
    if let Ok(d) = parse_csv_date(s) {
        let dt = d.and_hms_opt(0, 0, 0).unwrap();
        return Ok(Utc.from_utc_datetime(&dt));
    }

    Err(IngestError::Parse(format!("could not parse CSV datetime '{s}'")))
}

/// Parse a number that may have a leading `$`, commas, or whitespace.
fn parse_csv_number(s: &str) -> Result<f64, IngestError> {
    let cleaned = s.trim().replace(['$', ',', ' '], "");
    if cleaned.is_empty() {
        return Ok(0.0);
    }
    cleaned
        .parse::<f64>()
        .map_err(|e| IngestError::Parse(format!("invalid CSV number '{s}': {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_oge_csv() {
        let csv_data = "\
Date,Usage (kWh),Cost ($),Type
03/15/2026,1245,$129.73,Electric
02/14/2026,1102,$118.50,Electric
";
        let dir = std::env::temp_dir();
        let path = dir.join("test_oge_billing.csv");
        std::fs::write(&path, csv_data).unwrap();

        let account_id = Uuid::new_v4();
        let bills = parse_oge_csv(&path, account_id).unwrap();
        assert_eq!(bills.len(), 2);

        let first = &bills[0];
        assert!((first.total_usage - 1245.0).abs() < 0.01);
        assert!((first.total_amount.value() - 129.73).abs() < 0.01);
        assert_eq!(first.usage_unit, "kWh");

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_parse_oge_usage_csv() {
        let csv_data = "\
Date,Usage (kWh)
2026-03-15 00:00,1.2
2026-03-15 01:00,0.8
2026-03-15 02:00,0.6
";
        let dir = std::env::temp_dir();
        let path = dir.join("test_oge_usage.csv");
        std::fs::write(&path, csv_data).unwrap();

        let source = ReadingSource::Meter(Uuid::new_v4());
        let readings = parse_oge_usage_csv(&path, source).unwrap();
        assert_eq!(readings.len(), 3);
        assert!((readings[0].value - 1.2).abs() < 0.001);
        assert!((readings[1].value - 0.8).abs() < 0.001);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_parse_csv_number() {
        assert!((parse_csv_number("$1,234.56").unwrap() - 1234.56).abs() < 0.001);
        assert!((parse_csv_number("42").unwrap() - 42.0).abs() < 0.001);
        assert!((parse_csv_number("").unwrap() - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_parse_csv_date_formats() {
        assert_eq!(
            parse_csv_date("3/15/2026").unwrap(),
            NaiveDate::from_ymd_opt(2026, 3, 15).unwrap()
        );
        assert_eq!(
            parse_csv_date("2026-03-15").unwrap(),
            NaiveDate::from_ymd_opt(2026, 3, 15).unwrap()
        );
        assert_eq!(
            parse_csv_date("Mar 15, 2026").unwrap(),
            NaiveDate::from_ymd_opt(2026, 3, 15).unwrap()
        );
    }

    #[test]
    fn test_parse_csv_datetime_formats() {
        let dt = parse_csv_datetime("2026-03-15 14:30:00").unwrap();
        assert_eq!(dt.date_naive(), NaiveDate::from_ymd_opt(2026, 3, 15).unwrap());

        let dt2 = parse_csv_datetime("3/15/2026 14:30").unwrap();
        assert_eq!(dt2.date_naive(), NaiveDate::from_ymd_opt(2026, 3, 15).unwrap());
    }
}
