use anyhow::{Context, Result};
use chrono::NaiveDate;
use comfy_table::{modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL, Cell, ContentArrangement, Table};
use sqlx::PgPool;


// ---------------------------------------------------------------------------
// Monthly report
// ---------------------------------------------------------------------------

/// Generate a comprehensive monthly report.
///
/// `month` is a string in `YYYY-MM` format.
///
/// Sections:
///   1. Header with month and site info
///   2. Per-utility summary: usage, cost, comparison to prior month & prior year
///   3. Weather summary: average temp, CDD, HDD, comparison to normal
///   4. Device highlights: top energy consumers (if reading data available)
///   5. Active experiments status
///   6. Recommendations preview (top 3)
pub async fn monthly_report(pool: &PgPool, month: &str) -> Result<()> {
    let (year, mon) = parse_month(month)?;
    let period_start = NaiveDate::from_ymd_opt(year, mon, 1)
        .context("Invalid month")?;
    let period_end = if mon == 12 {
        NaiveDate::from_ymd_opt(year + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(year, mon + 1, 1)
    }
    .context("Invalid month boundary")?;

    let prev_month_start = if mon == 1 {
        NaiveDate::from_ymd_opt(year - 1, 12, 1)
    } else {
        NaiveDate::from_ymd_opt(year, mon - 1, 1)
    }
    .context("Invalid prior month")?;

    let prior_year_start = NaiveDate::from_ymd_opt(year - 1, mon, 1)
        .context("Invalid prior year month")?;
    let prior_year_end = if mon == 12 {
        NaiveDate::from_ymd_opt(year, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(year - 1, mon + 1, 1)
    }
    .context("Invalid prior year month boundary")?;

    // ------------------------------------------------------------------
    // Site
    // ------------------------------------------------------------------
    let sites = lothal_db::site::list_sites(pool).await?;
    let site = sites
        .first()
        .context("No sites found. Run `lothal init` first.")?;

    let month_label = period_start.format("%B %Y").to_string();

    // ------------------------------------------------------------------
    // 1. Header
    // ------------------------------------------------------------------
    println!();
    println!("============================================================");
    println!("          MONTHLY ENERGY REPORT -- {}", month_label);
    println!("============================================================");
    println!("  Site:  {} ({}, {} {})", site.address, site.city, site.state, site.zip);
    println!("  Period: {} to {}", period_start, period_end);
    println!();

    // ------------------------------------------------------------------
    // 2. Utility summary
    // ------------------------------------------------------------------
    let accounts =
        lothal_db::bill::list_utility_accounts_by_site(pool, site.id).await?;

    if accounts.is_empty() {
        println!("No utility accounts found.");
    } else {
        println!("--- Utility Summary ---");
        println!();

        let mut table = Table::new();
        table
            .load_preset(UTF8_FULL)
            .apply_modifier(UTF8_ROUND_CORNERS)
            .set_content_arrangement(ContentArrangement::Dynamic)
            .set_header(vec![
                Cell::new("Utility"),
                Cell::new("Usage"),
                Cell::new("Unit"),
                Cell::new("Cost"),
                Cell::new("$/Day"),
                Cell::new("vs Prev Mo"),
                Cell::new("vs Last Yr"),
            ]);

        for account in &accounts {
            let bills_cur = lothal_db::bill::list_bills_by_account_and_range(
                pool,
                account.id,
                period_start,
                period_end,
            )
            .await?;

            let bills_prev = lothal_db::bill::list_bills_by_account_and_range(
                pool,
                account.id,
                prev_month_start,
                period_start,
            )
            .await?;

            let bills_year = lothal_db::bill::list_bills_by_account_and_range(
                pool,
                account.id,
                prior_year_start,
                prior_year_end,
            )
            .await?;

            let cur_usage: f64 = bills_cur.iter().map(|b| b.total_usage).sum();
            let cur_cost: f64 = bills_cur.iter().map(|b| b.total_amount.value()).sum();
            let cur_days: i64 = bills_cur.iter().map(|b| b.period.days()).sum();
            let daily_cost = if cur_days > 0 {
                format!("${:.2}", cur_cost / cur_days as f64)
            } else {
                "-".into()
            };

            let prev_usage: f64 = bills_prev.iter().map(|b| b.total_usage).sum();
            let vs_prev = if prev_usage > 0.0 {
                let pct = ((cur_usage - prev_usage) / prev_usage) * 100.0;
                format!("{:+.1}%", pct)
            } else {
                "-".into()
            };

            let year_usage: f64 = bills_year.iter().map(|b| b.total_usage).sum();
            let vs_year = if year_usage > 0.0 {
                let pct = ((cur_usage - year_usage) / year_usage) * 100.0;
                format!("{:+.1}%", pct)
            } else {
                "-".into()
            };

            let unit = bills_cur
                .first()
                .map(|b| b.usage_unit.as_str())
                .unwrap_or("-");

            table.add_row(vec![
                Cell::new(format!("{} ({})", account.provider_name, account.utility_type)),
                Cell::new(format!("{:.1}", cur_usage)),
                Cell::new(unit),
                Cell::new(format!("${:.2}", cur_cost)),
                Cell::new(daily_cost),
                Cell::new(vs_prev),
                Cell::new(vs_year),
            ]);
        }

        println!("{table}");
        println!();
    }

    // ------------------------------------------------------------------
    // 3. Weather summary
    // ------------------------------------------------------------------
    println!("--- Weather Summary ---");
    println!();

    let weather_days = lothal_db::weather::get_daily_weather_summaries(
        pool,
        site.id,
        period_start,
        period_end,
    )
    .await?;

    if weather_days.is_empty() {
        println!("  No weather data available for this month.");
    } else {
        let base_temp = 65.0;
        let avg_temp: f64 =
            weather_days.iter().map(|w| w.avg_temp_f).sum::<f64>() / weather_days.len() as f64;
        let min_temp = weather_days
            .iter()
            .map(|w| w.min_temp_f)
            .fold(f64::INFINITY, f64::min);
        let max_temp = weather_days
            .iter()
            .map(|w| w.max_temp_f)
            .fold(f64::NEG_INFINITY, f64::max);
        let total_cdd: f64 = weather_days
            .iter()
            .map(|w| (w.avg_temp_f - base_temp).max(0.0))
            .sum();
        let total_hdd: f64 = weather_days
            .iter()
            .map(|w| (base_temp - w.avg_temp_f).max(0.0))
            .sum();
        let avg_humidity: Option<f64> = {
            let vals: Vec<f64> = weather_days
                .iter()
                .filter_map(|w| w.avg_humidity_pct)
                .collect();
            if vals.is_empty() {
                None
            } else {
                Some(vals.iter().sum::<f64>() / vals.len() as f64)
            }
        };

        let mut wtable = Table::new();
        wtable
            .load_preset(UTF8_FULL)
            .apply_modifier(UTF8_ROUND_CORNERS)
            .set_content_arrangement(ContentArrangement::Dynamic)
            .set_header(vec![Cell::new("Metric"), Cell::new("Value")]);

        wtable.add_row(vec![
            Cell::new("Observation days"),
            Cell::new(weather_days.len()),
        ]);
        wtable.add_row(vec![
            Cell::new("Avg temperature"),
            Cell::new(format!("{avg_temp:.1} F")),
        ]);
        wtable.add_row(vec![
            Cell::new("Min / Max temperature"),
            Cell::new(format!("{min_temp:.1} F / {max_temp:.1} F")),
        ]);
        wtable.add_row(vec![
            Cell::new("Total cooling degree days"),
            Cell::new(format!("{total_cdd:.1}")),
        ]);
        wtable.add_row(vec![
            Cell::new("Total heating degree days"),
            Cell::new(format!("{total_hdd:.1}")),
        ]);
        if let Some(hum) = avg_humidity {
            wtable.add_row(vec![
                Cell::new("Avg humidity"),
                Cell::new(format!("{hum:.0}%")),
            ]);
        }

        // Compare to prior year same month.
        let prior_weather = lothal_db::weather::get_daily_weather_summaries(
            pool,
            site.id,
            prior_year_start,
            prior_year_end,
        )
        .await?;

        if !prior_weather.is_empty() {
            let prior_avg: f64 =
                prior_weather.iter().map(|w| w.avg_temp_f).sum::<f64>()
                    / prior_weather.len() as f64;
            let diff = avg_temp - prior_avg;
            wtable.add_row(vec![
                Cell::new("vs same month last year"),
                Cell::new(format!("{:+.1} F", diff)),
            ]);
        }

        println!("{wtable}");
    }
    println!();

    // ------------------------------------------------------------------
    // 4. Device highlights
    // ------------------------------------------------------------------
    println!("--- Device Highlights ---");
    println!();

    let structures =
        lothal_db::site::get_structures_by_site(pool, site.id).await?;

    let mut all_devices = Vec::new();
    for structure in &structures {
        let devices =
            lothal_db::device::list_devices_by_structure(pool, structure.id).await?;
        all_devices.extend(devices);
    }

    if all_devices.is_empty() {
        println!("  No devices registered.");
    } else {
        // Estimate monthly energy for each device and sort descending.
        let mut estimates: Vec<(&str, f64)> = all_devices
            .iter()
            .filter_map(|d| {
                let annual = d.estimated_annual_kwh()?;
                Some((d.name.as_str(), annual / 12.0))
            })
            .collect();

        estimates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        if estimates.is_empty() {
            println!("  No devices have wattage/runtime estimates for energy ranking.");
        } else {
            let mut dtable = Table::new();
            dtable
                .load_preset(UTF8_FULL)
                .apply_modifier(UTF8_ROUND_CORNERS)
                .set_content_arrangement(ContentArrangement::Dynamic)
                .set_header(vec![
                    Cell::new("#"),
                    Cell::new("Device"),
                    Cell::new("Est. Monthly kWh"),
                ]);

            for (i, (name, kwh)) in estimates.iter().take(10).enumerate() {
                dtable.add_row(vec![
                    Cell::new(i + 1),
                    Cell::new(*name),
                    Cell::new(format!("{kwh:.1}")),
                ]);
            }

            println!("{dtable}");
        }
    }
    println!();

    // ------------------------------------------------------------------
    // 5. Active experiments
    // ------------------------------------------------------------------
    println!("--- Active Experiments ---");
    println!();

    let experiments =
        lothal_db::experiment::list_experiments_by_site(pool, site.id).await?;
    let hypotheses =
        lothal_db::experiment::list_hypotheses_by_site(pool, site.id).await?;

    let hyp_map: std::collections::HashMap<uuid::Uuid, &str> = hypotheses
        .iter()
        .map(|h| (h.id, h.title.as_str()))
        .collect();

    let active: Vec<_> = experiments
        .iter()
        .filter(|e| {
            matches!(
                e.status,
                lothal_core::ontology::experiment::ExperimentStatus::Active
                    | lothal_core::ontology::experiment::ExperimentStatus::Planned
            )
        })
        .collect();

    if active.is_empty() {
        println!("  No active experiments.");
    } else {
        for exp in &active {
            let title = hyp_map
                .get(&exp.hypothesis_id)
                .copied()
                .unwrap_or("(untitled)");
            println!(
                "  - {} [{}] result period ends {}",
                title,
                exp.status,
                exp.result_period.end,
            );
        }
    }
    println!();

    // ------------------------------------------------------------------
    // 6. Recommendations preview (top 3)
    // ------------------------------------------------------------------
    println!("--- Top Recommendations ---");
    println!();

    let stored_recs =
        lothal_db::experiment::list_recommendations_by_site(pool, site.id).await?;

    if stored_recs.is_empty() {
        println!("  No recommendations generated yet. Run `lothal recommend` to generate.");
    } else {
        for (i, rec) in stored_recs.iter().take(3).enumerate() {
            println!(
                "  {}. {} -- save ${:.2}/yr (payback {:.1} yr)",
                i + 1,
                rec.title,
                rec.estimated_annual_savings.value(),
                rec.payback_years,
            );
        }
    }

    println!();
    println!("============================================================");
    println!("  End of report for {month_label}");
    println!("============================================================");

    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Parse a `YYYY-MM` string into (year, month).
fn parse_month(s: &str) -> Result<(i32, u32)> {
    let parts: Vec<&str> = s.split('-').collect();
    if parts.len() != 2 {
        anyhow::bail!("Expected YYYY-MM format, got '{s}'");
    }
    let year: i32 = parts[0]
        .parse()
        .context("Invalid year in month string")?;
    let month: u32 = parts[1]
        .parse()
        .context("Invalid month in month string")?;

    if !(1..=12).contains(&month) {
        anyhow::bail!("Month must be 1-12, got {month}");
    }

    Ok((year, month))
}
