use anyhow::{bail, Context, Result};
use comfy_table::{modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL, Cell, ContentArrangement, Table};
use sqlx::PgPool;

use lothal_engine::simulate::{self, SimulationResult};

// ---------------------------------------------------------------------------
// Pool pump swap simulation
// ---------------------------------------------------------------------------

/// Simulate swapping a pool pump to a variable-speed model.
///
/// The effective electricity rate is derived from the most recent electric bill.
pub async fn simulate_swap_pump(pool: &PgPool, current_hp: f64) -> Result<()> {
    let rate = get_effective_electric_rate(pool).await?;
    let current_hours = 8.0; // typical single-speed pump runtime

    println!("=== Pool Pump Swap Simulation ===");
    println!();
    println!("  Current pump:     {current_hp:.1} HP single-speed");
    println!("  Current runtime:  {current_hours:.0} hours/day");
    println!("  Electric rate:    ${rate:.4}/kWh");
    println!();

    let result = simulate::simulate_pool_pump_swap(current_hp, current_hours, rate);
    print_simulation_result(&result);

    Ok(())
}

// ---------------------------------------------------------------------------
// Rate change simulation
// ---------------------------------------------------------------------------

/// Simulate switching to a different rate plan.
///
/// Supported plan names: `"smarthours"`, `"tou"`, `"flat"`.
pub async fn simulate_rate_change(pool: &PgPool, to: &str) -> Result<()> {
    // Get current monthly usage from recent bills.
    let (monthly_kwh, current_rate) = get_recent_usage_and_rate(pool).await?;

    // Estimate peak percentage and rate parameters based on the target plan.
    let (peak_pct, peak_rate, off_peak_rate) = match to.to_lowercase().as_str() {
        "smarthours" | "smart_hours" => {
            // SmartHours: ~35% of usage falls in peak, premium peak rate
            (0.35, current_rate * 1.8, current_rate * 0.6)
        }
        "tou" | "time_of_use" | "time-of-use" => {
            // Standard TOU: ~40% peak
            (0.40, current_rate * 1.5, current_rate * 0.7)
        }
        "flat" => {
            // Flat rate -- no peak/off-peak distinction
            (0.0, current_rate, current_rate)
        }
        other => bail!(
            "Unknown rate plan: '{other}'. Supported: smarthours, tou, flat"
        ),
    };

    println!("=== Rate Plan Change Simulation ===");
    println!();
    println!("  Target plan:      {to}");
    println!("  Monthly usage:    {monthly_kwh:.0} kWh");
    println!("  Current rate:     ${current_rate:.4}/kWh (flat effective)");
    println!("  Estimated peak:   {:.0}% of usage", peak_pct * 100.0);
    println!("  Peak rate:        ${peak_rate:.4}/kWh");
    println!("  Off-peak rate:    ${off_peak_rate:.4}/kWh");
    println!();

    let result = simulate::simulate_tou_shift(
        monthly_kwh,
        peak_pct,
        current_rate,
        peak_rate,
        off_peak_rate,
    );
    print_simulation_result(&result);

    Ok(())
}

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

/// Retrieve the effective electric rate from the most recent electric bill.
async fn get_effective_electric_rate(pool: &PgPool) -> Result<f64> {
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
    let latest = bills
        .last()
        .context("No electric bills found")?;

    latest
        .effective_rate()
        .map(|r| r.value())
        .context("Latest bill has zero usage; cannot determine rate")
}

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

/// Pretty-print a `SimulationResult` from the engine.
fn print_simulation_result(result: &SimulationResult) {
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec![Cell::new("Metric"), Cell::new("Value")]);

    table.add_row(vec![
        Cell::new("Scenario"),
        Cell::new(&result.scenario_description),
    ]);
    table.add_row(vec![
        Cell::new("Current annual cost"),
        Cell::new(format!("${:.2}", result.current_annual_cost.value())),
    ]);
    table.add_row(vec![
        Cell::new("Projected annual cost"),
        Cell::new(format!("${:.2}", result.projected_annual_cost.value())),
    ]);
    table.add_row(vec![
        Cell::new("Annual savings"),
        Cell::new(format!("${:.2}", result.annual_savings.value())),
    ]);
    table.add_row(vec![
        Cell::new("Equipment cost"),
        Cell::new(format!("${:.2}", result.capex.value())),
    ]);

    let payback_str = if result.simple_payback_years.is_infinite() || result.simple_payback_years > 99.0
    {
        "N/A".to_string()
    } else {
        format!("{:.1} years", result.simple_payback_years)
    };
    table.add_row(vec![
        Cell::new("Simple payback"),
        Cell::new(payback_str),
    ]);

    println!("{table}");

    if !result.notes.is_empty() {
        println!();
        println!("Notes:");
        for note in &result.notes {
            println!("  - {note}");
        }
    }
}
