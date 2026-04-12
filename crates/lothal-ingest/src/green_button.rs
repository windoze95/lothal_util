use std::path::Path;

use chrono::{DateTime, TimeZone, Utc};
use serde::Deserialize;
use uuid::Uuid;

use lothal_core::{Bill, Reading, ReadingKind, ReadingSource, Usd};

use crate::IngestError;

// ---------------------------------------------------------------------------
// Green Button ESPI XML structures
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct Feed {
    #[serde(rename = "entry", default)]
    entries: Vec<Entry>,
}

#[derive(Debug, Deserialize)]
struct Entry {
    #[serde(rename = "content")]
    content: Option<Content>,
}

#[derive(Debug, Deserialize)]
struct Content {
    #[serde(rename = "IntervalBlock")]
    interval_block: Option<IntervalBlock>,
}

#[derive(Debug, Deserialize)]
struct IntervalBlock {
    #[serde(rename = "interval")]
    interval: Option<TimePeriod>,
    #[serde(rename = "IntervalReading", default)]
    readings: Vec<IntervalReading>,
}

#[derive(Debug, Deserialize)]
struct IntervalReading {
    #[serde(rename = "timePeriod")]
    time_period: TimePeriod,
    /// Value in Wh (watt-hours) per the ESPI standard.
    value: i64,
}

#[derive(Debug, Deserialize)]
struct TimePeriod {
    /// Seconds since the UNIX epoch.
    start: i64,
    /// Duration in seconds.
    duration: i64,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Parse a Green Button XML file and aggregate interval readings into monthly bills.
///
/// Each `IntervalBlock` typically spans one billing month. The sum of the
/// `IntervalReading` values (Wh) within a block is converted to kWh and used as
/// the bill's `total_usage`. Because Green Button data does not contain cost
/// information, the bill's `total_amount` is set to zero.
pub fn parse_green_button(path: &Path, account_id: Uuid) -> Result<Vec<Bill>, IngestError> {
    let xml = std::fs::read_to_string(path)?;
    let feed: Feed = quick_xml::de::from_str(&xml)?;

    let mut bills: Vec<Bill> = Vec::new();

    for entry in &feed.entries {
        let block = match entry.content.as_ref().and_then(|c| c.interval_block.as_ref()) {
            Some(b) => b,
            None => continue,
        };

        if block.readings.is_empty() {
            continue;
        }

        // Determine the block's overall time span.
        let (block_start, block_end) = if let Some(ref iv) = block.interval {
            let s = epoch_to_utc(iv.start);
            let e = epoch_to_utc(iv.start + iv.duration);
            (s, e)
        } else {
            // Derive from first/last readings.
            let first = &block.readings[0].time_period;
            let last = &block.readings[block.readings.len() - 1].time_period;
            (
                epoch_to_utc(first.start),
                epoch_to_utc(last.start + last.duration),
            )
        };

        let total_wh: i64 = block.readings.iter().map(|r| r.value).sum();
        let total_kwh = total_wh as f64 / 1000.0;

        let period_start = block_start.date_naive();
        let period_end = block_end.date_naive();

        let mut bill = Bill::new(
            account_id,
            period_start,
            period_end,
            period_end, // use end of period as statement date
            total_kwh,
            "kWh".to_string(),
            Usd::zero(),
        );
        bill.source_file = Some(path.display().to_string());
        bill.notes = Some(format!(
            "Imported from Green Button XML ({} interval readings)",
            block.readings.len()
        ));

        bills.push(bill);
    }

    if bills.is_empty() {
        return Err(IngestError::Parse(
            "no IntervalBlock entries found in Green Button file".into(),
        ));
    }

    Ok(bills)
}

/// Parse a Green Button XML file into fine-grained `Reading` objects.
///
/// Each `IntervalReading` becomes a single `Reading` with the value converted
/// from Wh to kWh.
pub fn parse_green_button_readings(
    path: &Path,
    source: ReadingSource,
) -> Result<Vec<Reading>, IngestError> {
    let xml = std::fs::read_to_string(path)?;
    let feed: Feed = quick_xml::de::from_str(&xml)?;

    let mut readings: Vec<Reading> = Vec::new();

    for entry in &feed.entries {
        let block = match entry.content.as_ref().and_then(|c| c.interval_block.as_ref()) {
            Some(b) => b,
            None => continue,
        };

        for ir in &block.readings {
            let time = epoch_to_utc(ir.time_period.start);
            let kwh = ir.value as f64 / 1000.0;
            readings.push(Reading::at(time, source, ReadingKind::ElectricKwh, kwh));
        }
    }

    if readings.is_empty() {
        return Err(IngestError::Parse(
            "no IntervalReading entries found in Green Button file".into(),
        ));
    }

    Ok(readings)
}

/// Convert a UNIX timestamp to a `DateTime<Utc>`.
fn epoch_to_utc(secs: i64) -> DateTime<Utc> {
    Utc.timestamp_opt(secs, 0)
        .single()
        .unwrap_or_else(|| DateTime::<Utc>::MIN_UTC)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal Green Button XML for testing.
    const SAMPLE_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns="http://www.w3.org/2005/Atom">
  <entry>
    <content>
      <IntervalBlock>
        <interval>
          <start>1740787200</start>
          <duration>2678400</duration>
        </interval>
        <IntervalReading>
          <timePeriod>
            <start>1740787200</start>
            <duration>3600</duration>
          </timePeriod>
          <value>1500</value>
        </IntervalReading>
        <IntervalReading>
          <timePeriod>
            <start>1740790800</start>
            <duration>3600</duration>
          </timePeriod>
          <value>2300</value>
        </IntervalReading>
      </IntervalBlock>
    </content>
  </entry>
</feed>"#;

    #[test]
    fn test_parse_green_button_bills() {
        let dir = std::env::temp_dir();
        let path = dir.join("test_green_button.xml");
        std::fs::write(&path, SAMPLE_XML).unwrap();

        let account_id = Uuid::new_v4();
        let bills = parse_green_button(&path, account_id).unwrap();
        assert_eq!(bills.len(), 1);

        let bill = &bills[0];
        // 1500 + 2300 = 3800 Wh = 3.8 kWh
        assert!((bill.total_usage - 3.8).abs() < 0.001);
        assert_eq!(bill.usage_unit, "kWh");
        assert_eq!(bill.total_amount.value(), 0.0);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_parse_green_button_readings() {
        let dir = std::env::temp_dir();
        let path = dir.join("test_green_button_readings.xml");
        std::fs::write(&path, SAMPLE_XML).unwrap();

        let source = ReadingSource::Meter(Uuid::new_v4());
        let readings = parse_green_button_readings(&path, source).unwrap();
        assert_eq!(readings.len(), 2);
        // 1500 Wh = 1.5 kWh
        assert!((readings[0].value - 1.5).abs() < 0.001);
        // 2300 Wh = 2.3 kWh
        assert!((readings[1].value - 2.3).abs() < 0.001);

        std::fs::remove_file(&path).ok();
    }
}
