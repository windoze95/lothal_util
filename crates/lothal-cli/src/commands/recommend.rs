use anyhow::{Context, Result};
use comfy_table::{modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL, Cell, ContentArrangement, Table};
use sqlx::PgPool;

use lothal_engine::recommend;

// ---------------------------------------------------------------------------
// Generate recommendations
// ---------------------------------------------------------------------------

/// Build a `SiteContext` from the database, generate efficiency
/// recommendations, rank them, and display as a prioritized table.
pub async fn generate_recommendations(pool: &PgPool) -> Result<()> {
    println!("=== Generating Efficiency Recommendations ===");
    println!();

    // ----- Gather site context -----
    let sites = lothal_db::site::list_sites(pool).await?;
    let site = sites
        .first()
        .context("No sites found. Run `lothal init` first.")?;

    println!("Site: {} ({})", site.address, site.city);

    let accounts =
        lothal_db::bill::list_utility_accounts_by_site(pool, site.id).await?;
    let structures =
        lothal_db::site::get_structures_by_site(pool, site.id).await?;

    // Collect all devices across structures.
    let mut all_devices = Vec::new();
    for structure in &structures {
        let devices =
            lothal_db::device::list_devices_by_structure(pool, structure.id).await?;
        all_devices.extend(devices);
    }

    // Collect electric bills.
    let electric_account = accounts
        .iter()
        .find(|a| a.utility_type == lothal_core::ontology::utility::UtilityType::Electric);

    let bills = if let Some(acct) = electric_account {
        lothal_db::bill::list_bills_by_account(pool, acct.id).await?
    } else {
        Vec::new()
    };

    // Attempt to compute a baseline model for context.
    let baseline_model = if let Some(acct) = electric_account {
        try_compute_baseline(pool, acct.id, site.id).await
    } else {
        None
    };

    println!("  Utility accounts: {}", accounts.len());
    println!("  Structures:       {}", structures.len());
    println!("  Devices:          {}", all_devices.len());
    println!("  Bills:            {}", bills.len());
    println!(
        "  Baseline model:   {}",
        if baseline_model.is_some() { "available" } else { "not available" },
    );
    println!();

    // ----- Build SiteContext and generate -----
    let year_built = structures.first().and_then(|s| s.year_built);
    let has_pool = structures.iter().any(|s| s.has_pool);
    let climate_zone = site.climate_zone.clone();

    // Fetch property operations context for expanded recommendations.
    let pools = lothal_db::water::list_pools_by_site(pool, site.id).await?;
    let water_sources = lothal_db::water::list_water_sources_by_site(pool, site.id).await?;
    let septic = lothal_db::water::get_septic_system(pool, site.id).await?;
    let flocks = lothal_db::livestock::list_flocks_by_site(pool, site.id).await?;

    let ctx = recommend::SiteContext {
        site_id: site.id,
        year_built,
        has_pool,
        climate_zone,
        devices: all_devices,
        recent_bills: bills,
        baseline: baseline_model,
        pools,
        water_sources,
        septic,
        flocks,
    };

    let mut recs = recommend::generate_recommendations(&ctx);

    if recs.is_empty() {
        println!("No recommendations generated. Add more devices and bills for better results.");
        return Ok(());
    }

    recommend::rank_recommendations(&mut recs);

    // ----- Summary table -----
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec![
            Cell::new("#"),
            Cell::new("Title"),
            Cell::new("Category"),
            Cell::new("Annual Savings"),
            Cell::new("Capex"),
            Cell::new("Payback"),
            Cell::new("Confidence"),
        ]);

    for (i, rec) in recs.iter().enumerate() {
        let payback = if rec.payback_years.is_infinite() || rec.payback_years > 99.0 {
            "N/A".to_string()
        } else if rec.payback_years < 0.1 {
            "Immediate".to_string()
        } else {
            format!("{:.1} yr", rec.payback_years)
        };

        let confidence = format!("{:.0}%", rec.confidence * 100.0);

        table.add_row(vec![
            Cell::new(i + 1),
            Cell::new(truncate(&rec.title, 35)),
            Cell::new(rec.category.to_string()),
            Cell::new(format!("${:.2}", rec.estimated_annual_savings.value())),
            Cell::new(format!("${:.2}", rec.estimated_capex.value())),
            Cell::new(payback),
            Cell::new(confidence),
        ]);
    }

    println!("{table}");
    println!();

    // ----- Detailed view for top recommendations -----
    let top_n = 3.min(recs.len());
    println!("=== Top {} Recommendations (Detail) ===", top_n);
    println!();

    for (i, rec) in recs.iter().take(top_n).enumerate() {
        println!(
            "{}. {} [{}]",
            i + 1,
            rec.title,
            rec.category,
        );
        println!("   {}", rec.description);
        println!(
            "   Savings: ${:.2}/yr | Capex: ${:.2} | Payback: {}",
            rec.estimated_annual_savings.value(),
            rec.estimated_capex.value(),
            if rec.payback_years.is_infinite() || rec.payback_years > 99.0 {
                "N/A".to_string()
            } else {
                format!("{:.1} years", rec.payback_years)
            },
        );
        if let Some(ref reqs) = rec.data_requirements {
            println!("   Data needed: {reqs}");
        }
        println!();
    }

    println!("{} total recommendation(s)", recs.len());

    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Attempt to compute a baseline model from bills + weather. Returns None
/// on failure rather than propagating the error.
async fn try_compute_baseline(
    pool: &PgPool,
    account_id: uuid::Uuid,
    site_id: uuid::Uuid,
) -> Option<lothal_engine::baseline::BaselineModel> {
    let bills = lothal_db::bill::list_bills_by_account(pool, account_id)
        .await
        .ok()?;
    if bills.is_empty() {
        return None;
    }

    let earliest = bills.iter().map(|b| b.period.range.start).min()?;
    let latest = bills.iter().map(|b| b.period.range.end).max()?;

    let weather = lothal_db::weather::get_daily_weather_summaries(pool, site_id, earliest, latest)
        .await
        .ok()?;

    let weather_map: std::collections::HashMap<chrono::NaiveDate, &lothal_db::weather::DailyWeatherRow> =
        weather.iter().map(|w| (w.date, w)).collect();

    let base_temp = 65.0;
    let mut data_points = Vec::new();

    for bill in &bills {
        let daily_usage = bill.daily_usage()?;
        for date in bill.period.range.iter_days() {
            if let Some(w) = weather_map.get(&date) {
                let cdd = (w.avg_temp_f - base_temp).max(0.0);
                let hdd = (base_temp - w.avg_temp_f).max(0.0);
                data_points.push(lothal_engine::baseline::DailyDataPoint {
                    date,
                    usage: daily_usage,
                    cooling_degree_days: cdd,
                    heating_degree_days: hdd,
                });
            }
        }
    }

    let cooling_data: Vec<_> = data_points
        .iter()
        .filter(|d| d.cooling_degree_days > 0.0)
        .cloned()
        .collect();

    lothal_engine::baseline::compute_baseline(
        &cooling_data,
        lothal_engine::baseline::BaselineMode::Cooling,
    )
    .ok()
}

/// Truncate a string to at most `max` characters.
fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max.saturating_sub(3)])
    }
}
