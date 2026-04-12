use anyhow::{bail, Context, Result};
use chrono::{NaiveDate, Utc};
use comfy_table::{modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL, Cell, ContentArrangement, Table};
use sqlx::PgPool;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Query readings
// ---------------------------------------------------------------------------

/// Query and display device readings for the given duration string.
///
/// `device_id` is parsed as a UUID.
/// `last` is a human-readable duration like `"7d"`, `"24h"`, or `"30d"`.
pub async fn query_readings(pool: &PgPool, device_id: &str, last: &str) -> Result<()> {
    let id: Uuid = device_id
        .parse()
        .context("Invalid device UUID")?;

    let duration = parse_duration(last)?;
    let now = Utc::now();
    let start = now - duration;

    // The DB helper requires a specific kind, so we query each known kind and
    // merge results into one sorted list.
    let kinds = [
        "electric_kwh",
        "electric_watts",
        "gas_therms",
        "water_gallons",
        "temperature_f",
        "humidity_pct",
        "runtime_minutes",
    ];

    let mut all_readings = Vec::new();
    for kind in &kinds {
        let mut batch = lothal_db::reading::get_readings(pool, "device", id, kind, start, now)
            .await?;
        all_readings.append(&mut batch);
    }

    // Sort by time.
    all_readings.sort_by_key(|r| r.time);

    if all_readings.is_empty() {
        println!("No readings found for device {device_id} in the last {last}");
        return Ok(());
    }

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec![
            Cell::new("Time"),
            Cell::new("Kind"),
            Cell::new("Value"),
        ]);

    for reading in &all_readings {
        table.add_row(vec![
            Cell::new(reading.time.format("%Y-%m-%d %H:%M:%S")),
            Cell::new(reading.kind.to_string()),
            Cell::new(format!("{:.2}", reading.value)),
        ]);
    }

    println!("Readings for device {device_id} (last {last})");
    println!("{table}");
    println!("{} readings", all_readings.len());

    Ok(())
}

// ---------------------------------------------------------------------------
// Query bills
// ---------------------------------------------------------------------------

/// Query and display bills for the given utility account, optionally filtered
/// by year.
pub async fn query_bills(pool: &PgPool, account_id: &str, year: Option<i32>) -> Result<()> {
    let id: Uuid = account_id
        .parse()
        .context("Invalid account UUID")?;

    let bills = match year {
        Some(y) => {
            let start = NaiveDate::from_ymd_opt(y, 1, 1)
                .context("Invalid year")?;
            let end = NaiveDate::from_ymd_opt(y + 1, 1, 1)
                .context("Invalid year")?;
            lothal_db::bill::list_bills_by_account_and_range(pool, id, start, end).await?
        }
        None => lothal_db::bill::list_bills_by_account(pool, id).await?,
    };

    if bills.is_empty() {
        let suffix = year.map(|y| format!(" in {y}")).unwrap_or_default();
        println!("No bills found for account {account_id}{suffix}");
        return Ok(());
    }

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec![
            Cell::new("Period"),
            Cell::new("Days"),
            Cell::new("Usage"),
            Cell::new("Unit"),
            Cell::new("Amount"),
            Cell::new("$/Unit"),
            Cell::new("$/Day"),
        ]);

    let mut total_usage = 0.0;
    let mut total_amount = 0.0;

    for bill in &bills {
        let days = bill.period.days();
        let eff_rate = bill
            .effective_rate()
            .map(|r| format!("${:.4}", r.value()))
            .unwrap_or_else(|| "-".into());
        let daily_cost = bill
            .daily_cost()
            .map(|c| format!("${:.2}", c.value()))
            .unwrap_or_else(|| "-".into());

        table.add_row(vec![
            Cell::new(format!(
                "{} - {}",
                bill.period.range.start.format("%Y-%m-%d"),
                bill.period.range.end.format("%Y-%m-%d"),
            )),
            Cell::new(days),
            Cell::new(format!("{:.1}", bill.total_usage)),
            Cell::new(&bill.usage_unit),
            Cell::new(format!("${:.2}", bill.total_amount.value())),
            Cell::new(eff_rate),
            Cell::new(daily_cost),
        ]);

        total_usage += bill.total_usage;
        total_amount += bill.total_amount.value();
    }

    let header = match year {
        Some(y) => format!("Bills for account {} ({})", account_id, y),
        None => format!("Bills for account {}", account_id),
    };
    println!("{header}");
    println!("{table}");
    println!();
    println!(
        "Totals: {:.1} {} | ${:.2}",
        total_usage,
        bills.first().map(|b| b.usage_unit.as_str()).unwrap_or(""),
        total_amount,
    );
    println!("{} bills", bills.len());

    Ok(())
}

// ---------------------------------------------------------------------------
// Duration parser
// ---------------------------------------------------------------------------

/// Parse a shorthand duration string into a `chrono::Duration`.
///
/// Supported formats: `"7d"`, `"24h"`, `"30d"`, `"2w"`, `"6m"`.
fn parse_duration(s: &str) -> Result<chrono::Duration> {
    let s = s.trim();
    if s.is_empty() {
        bail!("Duration string is empty");
    }

    let (digits, suffix) = s.split_at(
        s.find(|c: char| !c.is_ascii_digit())
            .unwrap_or(s.len()),
    );

    let n: i64 = digits
        .parse()
        .context("Duration must start with a number (e.g. 7d, 24h)")?;

    let duration = match suffix.to_lowercase().as_str() {
        "h" | "hr" | "hrs" | "hour" | "hours" => chrono::Duration::hours(n),
        "d" | "day" | "days" => chrono::Duration::days(n),
        "w" | "wk" | "weeks" => chrono::Duration::weeks(n),
        "m" | "mo" | "month" | "months" => chrono::Duration::days(n * 30),
        "" => {
            // If bare number, assume days.
            chrono::Duration::days(n)
        }
        other => bail!("Unknown duration suffix: '{other}'. Use h, d, w, or m."),
    };

    Ok(duration)
}
