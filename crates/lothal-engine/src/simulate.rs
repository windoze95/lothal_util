//! "What if" scenario simulation engine.
//!
//! Lets users model the financial impact of device swaps, rate changes,
//! thermostat setpoint adjustments, and load shifting before committing
//! to a change.

use serde::{Deserialize, Serialize};

use lothal_core::Usd;

use crate::baseline::BaselineModel;
use crate::EngineError;

// ---------------------------------------------------------------------------
// Scenario types
// ---------------------------------------------------------------------------

/// A simplified device profile for simulation inputs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceProfile {
    pub name: String,
    /// Nameplate power draw in watts.
    pub nameplate_watts: f64,
    /// Typical daily runtime in hours.
    pub daily_run_hours: f64,
    /// Efficiency factor: 1.0 = full nameplate draw, lower = more efficient
    /// (e.g. 0.3 for a variable-speed pump running at low speed most of the time).
    pub efficiency_factor: f64,
}

impl DeviceProfile {
    /// Annual energy consumption in kWh.
    pub fn annual_kwh(&self) -> f64 {
        self.nameplate_watts * self.daily_run_hours * self.efficiency_factor * 365.0 / 1000.0
    }
}

/// Simplified flat-rate schedule for quick comparisons.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateScheduleProfile {
    pub name: String,
    pub base_charge: f64,
    pub rate_per_kwh: f64,
}

/// Direction of a thermostat setpoint change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SetpointDirection {
    /// Raising the cooling setpoint (saves energy in summer).
    Warmer,
    /// Lowering the heating setpoint (saves energy in winter).
    Cooler,
}

/// A simulation scenario.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Scenario {
    /// Swap one device for another and compare energy costs.
    DeviceSwap {
        current_device: DeviceProfile,
        new_device: DeviceProfile,
    },
    /// Compare two rate structures applied to the same usage.
    RateChange {
        current_schedule: RateScheduleProfile,
        new_schedule: RateScheduleProfile,
        /// Monthly usage in kWh.
        monthly_usage: f64,
    },
    /// Model the impact of adjusting the thermostat setpoint.
    /// Rule of thumb: each 1 degF saves ~3% on cooling, ~1% on heating.
    SetpointChange {
        direction: SetpointDirection,
        degrees_f: f64,
        baseline: BaselineModel,
    },
    /// Shift a percentage of peak load to off-peak hours (TOU optimization).
    LoadShift {
        peak_usage_kwh: f64,
        off_peak_rate: f64,
        peak_rate: f64,
        /// Fraction of peak load to move off-peak (0.0 - 1.0).
        shift_pct: f64,
    },
    // --- Property operations scenarios ---
    /// Model the ROI of installing a rainwater cistern.
    CisternInstall {
        roof_sqft: f64,
        annual_rainfall_inches: f64,
        municipal_cost_per_gallon: f64,
        cistern_cost: f64,
    },
    /// Model the savings from adding a pool cover.
    PoolCoverInstall {
        pool_surface_sqft: f64,
        daily_evaporation_gallons: f64,
        cover_cost: f64,
    },
    /// Model the economics of expanding a flock.
    FlockExpansion {
        current_birds: i32,
        additional_birds: i32,
        feed_cost_per_bird_monthly: f64,
        egg_value_per_bird_monthly: f64,
    },
}

// ---------------------------------------------------------------------------
// Result
// ---------------------------------------------------------------------------

/// The output of a simulation run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationResult {
    pub scenario_description: String,
    pub current_annual_cost: Usd,
    pub projected_annual_cost: Usd,
    pub annual_savings: Usd,
    /// Capital expenditure required (0 if none).
    pub capex: Usd,
    /// Simple payback = capex / annual_savings. `f64::INFINITY` when savings <= 0.
    pub simple_payback_years: f64,
    pub notes: Vec<String>,
}

// ---------------------------------------------------------------------------
// Core simulation
// ---------------------------------------------------------------------------

/// Run a scenario simulation.
///
/// `rate_per_kwh` is used for device-swap and setpoint scenarios that need
/// to convert kWh deltas into dollar amounts.
pub fn simulate(scenario: &Scenario, rate_per_kwh: f64) -> Result<SimulationResult, EngineError> {
    match scenario {
        Scenario::DeviceSwap {
            current_device,
            new_device,
        } => simulate_device_swap(current_device, new_device, rate_per_kwh),

        Scenario::RateChange {
            current_schedule,
            new_schedule,
            monthly_usage,
        } => simulate_rate_change(current_schedule, new_schedule, *monthly_usage),

        Scenario::SetpointChange {
            direction,
            degrees_f,
            baseline,
        } => simulate_setpoint_change(*direction, *degrees_f, baseline, rate_per_kwh),

        Scenario::LoadShift {
            peak_usage_kwh,
            off_peak_rate,
            peak_rate,
            shift_pct,
        } => simulate_load_shift(*peak_usage_kwh, *off_peak_rate, *peak_rate, *shift_pct),

        Scenario::CisternInstall {
            roof_sqft,
            annual_rainfall_inches,
            municipal_cost_per_gallon,
            cistern_cost,
        } => simulate_cistern(*roof_sqft, *annual_rainfall_inches, *municipal_cost_per_gallon, *cistern_cost),

        Scenario::PoolCoverInstall {
            pool_surface_sqft,
            daily_evaporation_gallons,
            cover_cost,
        } => simulate_pool_cover(*pool_surface_sqft, *daily_evaporation_gallons, *cover_cost),

        Scenario::FlockExpansion {
            current_birds,
            additional_birds,
            feed_cost_per_bird_monthly,
            egg_value_per_bird_monthly,
        } => simulate_flock_expansion(
            *current_birds,
            *additional_birds,
            *feed_cost_per_bird_monthly,
            *egg_value_per_bird_monthly,
        ),
    }
}

fn simulate_device_swap(
    current: &DeviceProfile,
    new: &DeviceProfile,
    rate: f64,
) -> Result<SimulationResult, EngineError> {
    let current_kwh = current.annual_kwh();
    let new_kwh = new.annual_kwh();
    let current_cost = current_kwh * rate;
    let new_cost = new_kwh * rate;
    let savings = current_cost - new_cost;
    let payback = if savings > 0.0 { 0.0 / savings } else { f64::INFINITY };

    Ok(SimulationResult {
        scenario_description: format!(
            "Replace {} with {}",
            current.name, new.name
        ),
        current_annual_cost: Usd::new(current_cost),
        projected_annual_cost: Usd::new(new_cost),
        annual_savings: Usd::new(savings),
        capex: Usd::zero(),
        simple_payback_years: payback,
        notes: vec![
            format!("Current annual usage: {current_kwh:.0} kWh"),
            format!("Projected annual usage: {new_kwh:.0} kWh"),
        ],
    })
}

fn simulate_rate_change(
    current: &RateScheduleProfile,
    new: &RateScheduleProfile,
    monthly_usage: f64,
) -> Result<SimulationResult, EngineError> {
    let annual_usage = monthly_usage * 12.0;
    let current_annual = current.base_charge * 12.0 + annual_usage * current.rate_per_kwh;
    let new_annual = new.base_charge * 12.0 + annual_usage * new.rate_per_kwh;
    let savings = current_annual - new_annual;

    Ok(SimulationResult {
        scenario_description: format!(
            "Switch from {} to {}",
            current.name, new.name
        ),
        current_annual_cost: Usd::new(current_annual),
        projected_annual_cost: Usd::new(new_annual),
        annual_savings: Usd::new(savings),
        capex: Usd::zero(),
        simple_payback_years: 0.0,
        notes: vec![
            format!("Based on {monthly_usage:.0} kWh/month ({annual_usage:.0} kWh/year)"),
        ],
    })
}

fn simulate_setpoint_change(
    direction: SetpointDirection,
    degrees_f: f64,
    baseline: &BaselineModel,
    rate: f64,
) -> Result<SimulationResult, EngineError> {
    // Rule of thumb: each 1 degF saves ~3% cooling, ~1% heating.
    let pct_per_degree = match direction {
        SetpointDirection::Warmer => 0.03,
        SetpointDirection::Cooler => 0.01,
    };

    // The weather-dependent portion of usage is (slope * avg_dd).  We estimate
    // an average day from the model's own training data.  With no access to the
    // original degree-day distribution we approximate total annual
    // weather-dependent cost via the slope contribution.
    //
    // A rough estimate: use base_load as the weather-independent portion and
    // assume the slope component over a year equals
    //   slope * (total_annual_dd)
    // We don't have total_annual_dd, so we estimate from total annual usage:
    //   annual_usage ≈ base_load * 365 + slope * total_annual_dd
    // and savings = pct_per_degree * degrees_f * (annual_usage - base_load*365)
    //
    // We use a typical Oklahoma total: ~1,800 CDD or ~3,600 HDD per year.
    let typical_annual_dd = match direction {
        SetpointDirection::Warmer => 1800.0,
        SetpointDirection::Cooler => 3600.0,
    };

    let weather_dependent_kwh = baseline.slope * typical_annual_dd;
    let base_kwh = baseline.base_load_kwh_per_day * 365.0;
    let total_kwh = base_kwh + weather_dependent_kwh;
    let savings_pct = pct_per_degree * degrees_f;
    let kwh_saved = weather_dependent_kwh * savings_pct;

    let current_cost = total_kwh * rate;
    let projected_cost = (total_kwh - kwh_saved) * rate;
    let dollar_savings = kwh_saved * rate;

    let label = match direction {
        SetpointDirection::Warmer => "warmer (cooling savings)",
        SetpointDirection::Cooler => "cooler (heating savings)",
    };

    Ok(SimulationResult {
        scenario_description: format!(
            "Adjust thermostat {degrees_f:.0}°F {label}"
        ),
        current_annual_cost: Usd::new(current_cost),
        projected_annual_cost: Usd::new(projected_cost),
        annual_savings: Usd::new(dollar_savings),
        capex: Usd::zero(),
        simple_payback_years: 0.0,
        notes: vec![
            format!("Weather-dependent portion: {weather_dependent_kwh:.0} kWh/yr"),
            format!("Estimated reduction: {:.1}%", savings_pct * 100.0),
            format!("Estimated kWh saved: {kwh_saved:.0}"),
        ],
    })
}

fn simulate_load_shift(
    peak_usage_kwh: f64,
    off_peak_rate: f64,
    peak_rate: f64,
    shift_pct: f64,
) -> Result<SimulationResult, EngineError> {
    if !(0.0..=1.0).contains(&shift_pct) {
        return Err(EngineError::InvalidInput(
            "shift_pct must be between 0.0 and 1.0".into(),
        ));
    }

    let shifted_kwh = peak_usage_kwh * shift_pct;
    let remaining_peak = peak_usage_kwh - shifted_kwh;

    let current_cost = peak_usage_kwh * peak_rate;
    let projected_cost = remaining_peak * peak_rate + shifted_kwh * off_peak_rate;
    let savings = current_cost - projected_cost;

    Ok(SimulationResult {
        scenario_description: format!(
            "Shift {:.0}% of peak load ({shifted_kwh:.0} kWh) to off-peak",
            shift_pct * 100.0
        ),
        current_annual_cost: Usd::new(current_cost),
        projected_annual_cost: Usd::new(projected_cost),
        annual_savings: Usd::new(savings),
        capex: Usd::zero(),
        simple_payback_years: 0.0,
        notes: vec![
            format!("Peak rate: ${peak_rate:.4}/kWh, Off-peak: ${off_peak_rate:.4}/kWh"),
            format!("Shifted: {shifted_kwh:.0} kWh, Remaining peak: {remaining_peak:.0} kWh"),
        ],
    })
}

// ---------------------------------------------------------------------------
// Property operations simulations
// ---------------------------------------------------------------------------

fn simulate_cistern(
    roof_sqft: f64,
    annual_rainfall_inches: f64,
    municipal_cost_per_gallon: f64,
    cistern_cost: f64,
) -> Result<SimulationResult, EngineError> {
    // 1 inch of rain on 1 sqft = 0.623 gallons
    let capturable_gallons = roof_sqft * annual_rainfall_inches * 0.623;
    // Assume 80% capture efficiency
    let captured = capturable_gallons * 0.80;
    let annual_savings = captured * municipal_cost_per_gallon;
    let payback = if annual_savings > 0.0 {
        cistern_cost / annual_savings
    } else {
        f64::INFINITY
    };

    Ok(SimulationResult {
        scenario_description: format!(
            "Install rainwater cistern ({roof_sqft:.0} sqft roof, {annual_rainfall_inches:.0}\"/yr)"
        ),
        current_annual_cost: Usd::new(capturable_gallons * municipal_cost_per_gallon),
        projected_annual_cost: Usd::new((capturable_gallons - captured) * municipal_cost_per_gallon),
        annual_savings: Usd::new(annual_savings),
        capex: Usd::new(cistern_cost),
        simple_payback_years: payback,
        notes: vec![
            format!("Capturable rainfall: {capturable_gallons:.0} gal/year"),
            format!("Captured at 80% efficiency: {captured:.0} gal/year"),
        ],
    })
}

fn simulate_pool_cover(
    pool_surface_sqft: f64,
    daily_evaporation_gallons: f64,
    cover_cost: f64,
) -> Result<SimulationResult, EngineError> {
    // A pool cover reduces evaporation by ~60% and chemical use proportionally.
    let annual_evaporation = daily_evaporation_gallons * 365.0;
    let saved_gallons = annual_evaporation * 0.60;
    // Water cost savings + chemical savings (~$0.005/gallon effective)
    let savings = saved_gallons * 0.005;
    let payback = if savings > 0.0 {
        cover_cost / savings
    } else {
        f64::INFINITY
    };

    Ok(SimulationResult {
        scenario_description: format!(
            "Install pool cover ({pool_surface_sqft:.0} sqft surface)"
        ),
        current_annual_cost: Usd::new(annual_evaporation * 0.005),
        projected_annual_cost: Usd::new((annual_evaporation - saved_gallons) * 0.005),
        annual_savings: Usd::new(savings),
        capex: Usd::new(cover_cost),
        simple_payback_years: payback,
        notes: vec![
            format!("Current annual evaporation: {annual_evaporation:.0} gallons"),
            format!("Saved with cover (60% reduction): {saved_gallons:.0} gallons"),
        ],
    })
}

fn simulate_flock_expansion(
    current_birds: i32,
    additional_birds: i32,
    feed_cost_per_bird_monthly: f64,
    egg_value_per_bird_monthly: f64,
) -> Result<SimulationResult, EngineError> {
    let total = current_birds + additional_birds;
    let current_annual_cost = current_birds as f64 * feed_cost_per_bird_monthly * 12.0;
    let new_annual_cost = total as f64 * feed_cost_per_bird_monthly * 12.0;
    let new_annual_revenue = total as f64 * egg_value_per_bird_monthly * 12.0;
    let current_annual_revenue = current_birds as f64 * egg_value_per_bird_monthly * 12.0;

    let incremental_cost = new_annual_cost - current_annual_cost;
    let incremental_revenue = new_annual_revenue - current_annual_revenue;
    let net_savings = incremental_revenue - incremental_cost;

    // Capex: ~$30-50 per bird for coop expansion, feeder, waterer
    let capex = additional_birds as f64 * 40.0;
    let payback = if net_savings > 0.0 {
        capex / net_savings
    } else {
        f64::INFINITY
    };

    Ok(SimulationResult {
        scenario_description: format!(
            "Expand flock from {current_birds} to {total} birds"
        ),
        current_annual_cost: Usd::new(current_annual_cost - current_annual_revenue),
        projected_annual_cost: Usd::new(new_annual_cost - new_annual_revenue),
        annual_savings: Usd::new(net_savings),
        capex: Usd::new(capex),
        simple_payback_years: payback,
        notes: vec![
            format!("Additional feed cost: ${incremental_cost:.0}/year"),
            format!("Additional egg value: ${incremental_revenue:.0}/year"),
            format!("Net annual benefit: ${net_savings:.0}"),
        ],
    })
}

