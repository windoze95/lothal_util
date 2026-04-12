use anyhow::{bail, Context, Result};
use comfy_table::{modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL, Cell, ContentArrangement, Table};
use sqlx::PgPool;

// ---------------------------------------------------------------------------
// Thermostat setpoint simulation
// ---------------------------------------------------------------------------

/// Simulate a thermostat setpoint adjustment.
///
/// `change` is the number of degrees to raise (positive) or lower (negative)
/// the setpoint. `season` must be `"summer"` or `"winter"`.
///
/// Rule of thumb: each 1 degree F of setpoint change saves roughly 3% of
/// heating/cooling cost (DOE estimate).
pub async fn simulate_setpoint(pool: &PgPool, change: f64, season: &str) -> Result<()> {
    let season_lower = season.to_lowercase();
    let is_summer = match season_lower.as_str() {
        "summer" | "cooling" => true,
        "winter" | "heating" => false,
        other => bail!("Unknown season: '{other}'. Use 'summer' or 'winter'."),
    };

    let (monthly_kwh, rate) = get_recent_usage_and_rate(pool).await?;

    // Estimate the HVAC fraction of total usage.
    let hvac_fraction = if is_summer { 0.50 } else { 0.40 };
    let hvac_monthly = monthly_kwh * hvac_fraction;

    // DOE rule of thumb: ~3% savings per degree F of setpoint change.
    // In summer, raising the setpoint saves; in winter, lowering it saves.
    let effective_change = if is_summer { change } else { -change };
    let savings_pct = (effective_change.abs() * 0.03).min(0.30); // cap at 30%
    let hvac_savings_monthly = hvac_monthly * savings_pct;
    let annual_savings = hvac_savings_monthly * 12.0;
    let annual_savings_usd = annual_savings * rate;
    let current_annual_cost = monthly_kwh * 12.0 * rate;
    let projected_annual_cost = current_annual_cost - annual_savings_usd;

    let direction = if (is_summer && change > 0.0) || (!is_summer && change < 0.0) {
        "saves energy"
    } else {
        "increases energy use"
    };

    println!("=== Thermostat Setpoint Simulation ===");
    println!();
    println!("  Season:           {season}");
    println!(
        "  Setpoint change:  {:+.1} deg F ({direction})",
        change,
    );
    println!("  Monthly usage:    {monthly_kwh:.0} kWh");
    println!(
        "  HVAC share:       {:.0}% (~{:.0} kWh/mo)",
        hvac_fraction * 100.0,
        hvac_monthly,
    );
    println!(
        "  Est. savings:     {:.1}% of HVAC ({:.0} kWh/mo)",
        savings_pct * 100.0,
        hvac_savings_monthly,
    );
    println!();

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec![Cell::new("Metric"), Cell::new("Value")]);

    table.add_row(vec![
        Cell::new("Current annual cost"),
        Cell::new(format!("${current_annual_cost:.2}")),
    ]);
    table.add_row(vec![
        Cell::new("Projected annual cost"),
        Cell::new(format!("${projected_annual_cost:.2}")),
    ]);
    table.add_row(vec![
        Cell::new("Annual savings"),
        Cell::new(format!("${annual_savings_usd:.2}")),
    ]);
    table.add_row(vec![
        Cell::new("Equipment cost"),
        Cell::new("$0.00 (behavioral)"),
    ]);
    table.add_row(vec![
        Cell::new("Payback period"),
        Cell::new("Immediate"),
    ]);

    println!("{table}");
    println!();

    println!("Notes:");
    println!("  - Based on DOE estimate of ~3% HVAC savings per degree F");
    println!("  - Actual savings depend on insulation, climate, and occupancy");
    if effective_change.abs() > 3.0 {
        println!("  - Large setpoint changes (>{:.0} deg) may reduce comfort", effective_change.abs());
    }
    println!("  - Consider a programmable/smart thermostat for scheduled setbacks");

    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Retrieve recent average monthly usage (kWh) and effective rate from the
/// last 12 months of electric bills.
async fn get_recent_usage_and_rate(pool: &PgPool) -> Result<(f64, f64)> {
    let sites = lothal_db::site::list_sites(pool).await?;
    let site = sites
        .first()
        .context("No sites found. Run `lothal init` first.")?;

    let accounts =
        lothal_db::bill::list_utility_accounts_by_site(pool, site.id).await?;
    let electric_account = accounts
        .iter()
        .find(|a| a.utility_type == lothal_core::ontology::utility::UtilityType::Electric)
        .context("No electric utility account found")?;

    let bills =
        lothal_db::bill::list_bills_by_account(pool, electric_account.id).await?;

    if bills.is_empty() {
        anyhow::bail!("No electric bills found");
    }

    // Use the last 12 bills (approximately one year).
    let recent: Vec<_> = bills.iter().rev().take(12).collect();

    let total_usage: f64 = recent.iter().map(|b| b.total_usage).sum();
    let total_cost: f64 = recent.iter().map(|b| b.total_amount.value()).sum();
    let n = recent.len() as f64;

    let avg_monthly_usage = total_usage / n;
    let effective_rate = if total_usage > 0.0 {
        total_cost / total_usage
    } else {
        0.10 // fallback
    };

    Ok((avg_monthly_usage, effective_rate))
}
