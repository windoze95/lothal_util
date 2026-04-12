use anyhow::{Context, Result};
use comfy_table::{modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL, Cell, ContentArrangement, Table};
use sqlx::PgPool;
use uuid::Uuid;

use lothal_engine::baseline::{
    self, BaselineMode, BaselineModel, BaselineSummary, DailyDataPoint,
};

// ---------------------------------------------------------------------------
// Baseline command
// ---------------------------------------------------------------------------

/// Compute and display a weather-normalized energy baseline for a utility
/// account.
///
/// Steps:
///   1. Parse the account UUID.
///   2. Fetch bills for the account.
///   3. Resolve the site and fetch daily weather summaries.
///   4. Build `DailyDataPoint` records by spreading bill usage across days and
///      pairing with weather degree-day data.
///   5. Compute baselines for both cooling and heating seasons.
///   6. Display model coefficients, R-squared, and a textual summary.
///   7. Print a simple ASCII chart of actual vs predicted when data permits.
pub async fn compute_baseline_cmd(pool: &PgPool, account_id: &str) -> Result<()> {
    let acct_id: Uuid = account_id
        .parse()
        .context("Invalid account UUID")?;

    // ------------------------------------------------------------------
    // 1. Fetch bills
    // ------------------------------------------------------------------
    println!("Loading bills for account {account_id}...");
    let bills = lothal_db::bill::list_bills_by_account(pool, acct_id).await?;
    if bills.is_empty() {
        println!("No bills found for account {account_id}. Import bills first.");
        return Ok(());
    }
    println!("  Found {} bills", bills.len());

    // ------------------------------------------------------------------
    // 2. Resolve the site from the utility account
    // ------------------------------------------------------------------
    let account = lothal_db::bill::get_utility_account(pool, acct_id)
        .await?
        .context("Utility account not found")?;
    let site_id = account.site_id;

    // ------------------------------------------------------------------
    // 3. Determine date range covered by bills, fetch daily weather
    // ------------------------------------------------------------------
    let earliest = bills
        .iter()
        .map(|b| b.period.range.start)
        .min()
        .unwrap();
    let latest = bills
        .iter()
        .map(|b| b.period.range.end)
        .max()
        .unwrap();

    println!("  Bill range: {earliest} to {latest}");
    println!("Loading weather data for site...");

    let weather_days =
        lothal_db::weather::get_daily_weather_summaries(pool, site_id, earliest, latest)
            .await?;
    println!("  Found {} daily weather summaries", weather_days.len());

    if weather_days.is_empty() {
        println!("\nNo weather data available for this period.");
        println!("Run `lothal weather fetch` to pull NWS observations first.");
        return Ok(());
    }

    // ------------------------------------------------------------------
    // 4. Build DailyDataPoint array
    // ------------------------------------------------------------------
    //
    // For each bill we compute an average daily usage and spread it across the
    // billing period days. Each day is then paired with the weather summary for
    // that date if one exists.
    let base_temp = 65.0_f64;
    let mut data_points: Vec<DailyDataPoint> = Vec::new();

    // Index weather by date for fast lookup.
    let weather_map: std::collections::HashMap<chrono::NaiveDate, &lothal_db::weather::DailyWeatherRow> =
        weather_days.iter().map(|w| (w.date, w)).collect();

    for bill in &bills {
        let daily_usage = match bill.daily_usage() {
            Some(u) => u,
            None => continue,
        };

        for date in bill.period.range.iter_days() {
            if let Some(w) = weather_map.get(&date) {
                let cdd = (w.avg_temp_f - base_temp).max(0.0);
                let hdd = (base_temp - w.avg_temp_f).max(0.0);
                data_points.push(DailyDataPoint {
                    date,
                    usage: daily_usage,
                    cooling_degree_days: cdd,
                    heating_degree_days: hdd,
                });
            }
        }
    }

    if data_points.len() < 3 {
        println!(
            "\nInsufficient paired data points ({}) to compute a baseline.",
            data_points.len()
        );
        println!("Need at least 3 days with both bill and weather data.");
        return Ok(());
    }

    println!("  Built {} daily data points", data_points.len());
    println!();

    // ------------------------------------------------------------------
    // 5. Compute baselines for both modes
    // ------------------------------------------------------------------
    // Cooling: only use days with CDD > 0
    let cooling_data: Vec<DailyDataPoint> = data_points
        .iter()
        .filter(|d| d.cooling_degree_days > 0.0)
        .cloned()
        .collect();

    let heating_data: Vec<DailyDataPoint> = data_points
        .iter()
        .filter(|d| d.heating_degree_days > 0.0)
        .cloned()
        .collect();

    let cooling_result = if cooling_data.len() >= 3 {
        match baseline::compute_baseline(&cooling_data, BaselineMode::Cooling) {
            Ok(model) => {
                let summary = baseline::summarize_baseline(
                    &model,
                    &cooling_data,
                    &format!("{earliest} to {latest} (cooling)"),
                );
                Some((model, summary, cooling_data))
            }
            Err(e) => {
                println!("Could not compute cooling baseline: {e}");
                None
            }
        }
    } else {
        println!("Not enough cooling-season data ({} days, need 3+)", cooling_data.len());
        None
    };

    let heating_result = if heating_data.len() >= 3 {
        match baseline::compute_baseline(&heating_data, BaselineMode::Heating) {
            Ok(model) => {
                let summary = baseline::summarize_baseline(
                    &model,
                    &heating_data,
                    &format!("{earliest} to {latest} (heating)"),
                );
                Some((model, summary, heating_data))
            }
            Err(e) => {
                println!("Could not compute heating baseline: {e}");
                None
            }
        }
    } else {
        println!("Not enough heating-season data ({} days, need 3+)", heating_data.len());
        None
    };

    // ------------------------------------------------------------------
    // 6. Display results
    // ------------------------------------------------------------------
    if let Some((ref model, ref summary, _)) = cooling_result {
        print_baseline_section("COOLING BASELINE", model, summary);
    }

    if let Some((ref model, ref summary, _)) = heating_result {
        print_baseline_section("HEATING BASELINE", model, summary);
    }

    // ------------------------------------------------------------------
    // 7. ASCII chart
    // ------------------------------------------------------------------
    if let Some((ref model, _, ref data)) = cooling_result {
        if data.len() >= 5 {
            println!();
            print_ascii_chart("Cooling: Actual vs Predicted (kWh/day)", model, data, BaselineMode::Cooling);
        }
    }

    if let Some((ref model, _, ref data)) = heating_result {
        if data.len() >= 5 {
            println!();
            print_ascii_chart("Heating: Actual vs Predicted (kWh/day)", model, data, BaselineMode::Heating);
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn print_baseline_section(title: &str, model: &BaselineModel, summary: &BaselineSummary) {
    println!("=== {title} ===");
    println!();

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec![Cell::new("Metric"), Cell::new("Value")]);

    table.add_row(vec![
        Cell::new("Slope (kWh per degree-day)"),
        Cell::new(format!("{:.4}", model.slope)),
    ]);
    table.add_row(vec![
        Cell::new("Intercept (base load kWh/day)"),
        Cell::new(format!("{:.2}", model.intercept)),
    ]);
    table.add_row(vec![
        Cell::new("R-squared"),
        Cell::new(format!("{:.4}", model.r_squared)),
    ]);
    table.add_row(vec![
        Cell::new("Data points"),
        Cell::new(model.data_points_count.to_string()),
    ]);
    table.add_row(vec![
        Cell::new("Avg daily actual (kWh)"),
        Cell::new(format!("{:.2}", summary.avg_daily_actual)),
    ]);
    table.add_row(vec![
        Cell::new("Avg daily predicted (kWh)"),
        Cell::new(format!("{:.2}", summary.avg_daily_predicted)),
    ]);
    table.add_row(vec![
        Cell::new("Total actual (kWh)"),
        Cell::new(format!("{:.1}", summary.total_actual)),
    ]);
    table.add_row(vec![
        Cell::new("Total predicted (kWh)"),
        Cell::new(format!("{:.1}", summary.total_predicted)),
    ]);

    println!("{table}");

    let r2 = model.r_squared;
    let interpretation = if r2 >= 0.85 {
        "Strong fit -- weather explains most usage variation"
    } else if r2 >= 0.6 {
        "Moderate fit -- weather is a significant but not sole driver"
    } else if r2 >= 0.3 {
        "Weak fit -- usage is only loosely correlated with weather"
    } else {
        "Poor fit -- usage appears largely independent of weather"
    };

    println!();
    println!("  Interpretation: {interpretation}");
    println!(
        "  Base load: {:.2} kWh/day (usage when degree-days = 0)",
        model.base_load_kwh_per_day,
    );
    println!("  Period: {}", summary.period_description);
    println!();
}

/// Print a simple ASCII chart comparing actual and predicted daily usage,
/// bucketed by degree-day ranges.
fn print_ascii_chart(
    title: &str,
    model: &BaselineModel,
    data: &[DailyDataPoint],
    mode: BaselineMode,
) {
    println!("{title}");
    println!("{}", "-".repeat(title.len()));

    // Collect (degree_day, actual, predicted) tuples.
    let mut points: Vec<(f64, f64, f64)> = data
        .iter()
        .map(|d| {
            let dd = match mode {
                BaselineMode::Cooling => d.cooling_degree_days,
                BaselineMode::Heating => d.heating_degree_days,
            };
            let predicted = model.slope * dd + model.intercept;
            (dd, d.usage, predicted)
        })
        .collect();

    points.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    let max_usage = points
        .iter()
        .flat_map(|(_, a, p)| [*a, *p])
        .fold(0.0_f64, f64::max);

    let chart_width = 50;

    let dd_label = match mode {
        BaselineMode::Cooling => "CDD",
        BaselineMode::Heating => "HDD",
    };

    println!("{:>6}  {:>6}  {:>6}  Chart", dd_label, "Actual", "Pred");

    for (dd, actual, predicted) in &points {
        let a_bar = if max_usage > 0.0 {
            ((actual / max_usage) * chart_width as f64).round() as usize
        } else {
            0
        };
        let p_bar = if max_usage > 0.0 {
            ((predicted / max_usage) * chart_width as f64).round() as usize
        } else {
            0
        };

        let actual_str = "#".repeat(a_bar.min(chart_width));
        let pred_str = "-".repeat(p_bar.min(chart_width));

        println!("{dd:6.1}  {actual:6.1}  {predicted:6.1}  {actual_str}");
        println!("{:6}  {:6}  {:6}  {pred_str}", "", "", "");
    }

    println!();
    println!("  # = actual    - = predicted");
}
