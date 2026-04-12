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
// Pre-built Oklahoma helpers
// ---------------------------------------------------------------------------

/// Simulate replacing a single-speed pool pump with a variable-speed
/// IntelliFlo3.
///
/// Variable-speed pumps run longer at much lower power, typically saving
/// 70-80% of energy vs a single-speed pump.
pub fn simulate_pool_pump_swap(
    current_hp: f64,
    current_hours: f64,
    rate_per_kwh: f64,
) -> SimulationResult {
    // 1 HP ≈ 746 W for a single-speed pump
    let current_watts = current_hp * 746.0;
    let current = DeviceProfile {
        name: format!("{current_hp:.1} HP single-speed pool pump"),
        nameplate_watts: current_watts,
        daily_run_hours: current_hours,
        efficiency_factor: 1.0,
    };

    // Variable-speed: runs ~12 hrs/day at much lower wattage.
    // Typical IntelliFlo3 draws ~300-500 W at low speed for an equivalent
    // turnover.  We model as same nameplate but 0.25 efficiency_factor
    // running 12 hrs/day.
    let new = DeviceProfile {
        name: "Pentair IntelliFlo3 VSF".to_string(),
        nameplate_watts: current_watts,
        daily_run_hours: 12.0,
        efficiency_factor: 0.25,
    };

    let current_kwh = current.annual_kwh();
    let new_kwh = new.annual_kwh();
    let savings = (current_kwh - new_kwh) * rate_per_kwh;
    let capex = 1500.0; // typical installed cost
    let payback = if savings > 0.0 {
        capex / savings
    } else {
        f64::INFINITY
    };

    SimulationResult {
        scenario_description: format!(
            "Replace {:.1} HP single-speed pool pump with IntelliFlo3 VSF",
            current_hp
        ),
        current_annual_cost: Usd::new(current_kwh * rate_per_kwh),
        projected_annual_cost: Usd::new(new_kwh * rate_per_kwh),
        annual_savings: Usd::new(savings),
        capex: Usd::new(capex),
        simple_payback_years: payback,
        notes: vec![
            format!("Current: {current_kwh:.0} kWh/yr"),
            format!("Projected: {new_kwh:.0} kWh/yr"),
            format!("Reduction: {:.0}%", (1.0 - new_kwh / current_kwh) * 100.0),
            "Variable speed runs longer at lower RPM for same turnover".to_string(),
        ],
    }
}

/// Simulate switching to OG&E SmartHours time-of-use rate.
///
/// * `monthly_kwh` — average monthly consumption
/// * `peak_pct` — fraction of usage currently during peak hours (2-7 PM weekdays, June-Sep)
/// * `flat_rate` — current flat $/kWh
/// * `peak_rate` — SmartHours peak $/kWh
/// * `off_peak_rate` — SmartHours off-peak $/kWh
pub fn simulate_tou_shift(
    monthly_kwh: f64,
    peak_pct: f64,
    flat_rate: f64,
    peak_rate: f64,
    off_peak_rate: f64,
) -> SimulationResult {
    let annual_kwh = monthly_kwh * 12.0;
    let peak_kwh = annual_kwh * peak_pct;
    let off_peak_kwh = annual_kwh * (1.0 - peak_pct);

    let current_annual = annual_kwh * flat_rate;
    let tou_annual = peak_kwh * peak_rate + off_peak_kwh * off_peak_rate;
    let savings = current_annual - tou_annual;

    let payback = 0.0; // no capex

    SimulationResult {
        scenario_description: "Switch to OG&E SmartHours TOU rate".to_string(),
        current_annual_cost: Usd::new(current_annual),
        projected_annual_cost: Usd::new(tou_annual),
        annual_savings: Usd::new(savings),
        capex: Usd::zero(),
        simple_payback_years: payback,
        notes: vec![
            format!("Annual usage: {annual_kwh:.0} kWh"),
            format!("Peak portion: {:.0}% ({peak_kwh:.0} kWh)", peak_pct * 100.0),
            format!("Flat rate: ${flat_rate:.4}/kWh"),
            format!("Peak: ${peak_rate:.4}/kWh, Off-peak: ${off_peak_rate:.4}/kWh"),
            "SmartHours peak window: 2-7 PM weekdays, June through September".to_string(),
        ],
    }
}

/// Simulate upgrading attic insulation.
///
/// Heat flow through the attic ∝ 1/R-value, so reducing 1/R reduces
/// the weather-dependent load proportionally.
///
/// * `current_r` — current R-value (e.g. R-19)
/// * `target_r` — target R-value (e.g. R-49)
/// * `attic_sqft` — attic area in square feet (used for cost estimate)
/// * `annual_cooling_kwh` — annual kWh attributable to cooling
pub fn simulate_insulation_upgrade(
    current_r: f64,
    target_r: f64,
    attic_sqft: f64,
    annual_cooling_kwh: f64,
) -> SimulationResult {
    // Fraction of heat gain reduction: 1 - (current_R / target_R)
    let reduction_fraction = 1.0 - (current_r / target_r);

    // Attic heat gain is a portion of total cooling load — roughly 25-35% for
    // a single-story Oklahoma home.  We use 30% as the estimate.
    let attic_share = 0.30;
    let kwh_saved = annual_cooling_kwh * attic_share * reduction_fraction;

    // Cost: ~$1.50-$2.50 per sqft for blown-in cellulose/fiberglass
    let cost_per_sqft = 2.00;
    let capex = attic_sqft * cost_per_sqft;

    // Typical Oklahoma rate
    let rate = 0.11;
    let dollar_savings = kwh_saved * rate;
    let current_cost = annual_cooling_kwh * rate;
    let projected_cost = current_cost - dollar_savings;
    let payback = if dollar_savings > 0.0 {
        capex / dollar_savings
    } else {
        f64::INFINITY
    };

    SimulationResult {
        scenario_description: format!(
            "Upgrade attic insulation from R-{current_r:.0} to R-{target_r:.0}"
        ),
        current_annual_cost: Usd::new(current_cost),
        projected_annual_cost: Usd::new(projected_cost),
        annual_savings: Usd::new(dollar_savings),
        capex: Usd::new(capex),
        simple_payback_years: payback,
        notes: vec![
            format!("Attic area: {attic_sqft:.0} sqft"),
            format!("Heat-flow reduction: {:.0}%", reduction_fraction * 100.0),
            format!("Attic share of cooling load: {:.0}%", attic_share * 100.0),
            format!("Estimated kWh saved: {kwh_saved:.0}"),
            format!("Installed cost estimate: ${capex:.0} (@ ${cost_per_sqft:.2}/sqft)"),
        ],
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_device_swap_simulation() {
        let current = DeviceProfile {
            name: "Old pump".into(),
            nameplate_watts: 1500.0,
            daily_run_hours: 8.0,
            efficiency_factor: 1.0,
        };
        let new = DeviceProfile {
            name: "New pump".into(),
            nameplate_watts: 1500.0,
            daily_run_hours: 12.0,
            efficiency_factor: 0.25,
        };
        let result = simulate(
            &Scenario::DeviceSwap {
                current_device: current,
                new_device: new,
            },
            0.11,
        )
        .unwrap();

        assert!(result.annual_savings.value() > 0.0);
        assert!(result.projected_annual_cost.value() < result.current_annual_cost.value());
    }

    #[test]
    fn test_rate_change_simulation() {
        let current = RateScheduleProfile {
            name: "OG&E Standard".into(),
            base_charge: 15.0,
            rate_per_kwh: 0.11,
        };
        let new = RateScheduleProfile {
            name: "OG&E SmartHours".into(),
            base_charge: 15.0,
            rate_per_kwh: 0.08,
        };
        let result = simulate(
            &Scenario::RateChange {
                current_schedule: current,
                new_schedule: new,
                monthly_usage: 1500.0,
            },
            0.11,
        )
        .unwrap();

        assert!(result.annual_savings.value() > 0.0);
    }

    #[test]
    fn test_load_shift_simulation() {
        let result = simulate(
            &Scenario::LoadShift {
                peak_usage_kwh: 5000.0,
                off_peak_rate: 0.05,
                peak_rate: 0.20,
                shift_pct: 0.40,
            },
            0.11,
        )
        .unwrap();

        // Shifting 2000 kWh from $0.20 to $0.05 saves 2000 * 0.15 = $300
        assert!((result.annual_savings.value() - 300.0).abs() < 0.01);
    }

    #[test]
    fn test_load_shift_invalid_pct() {
        let result = simulate(
            &Scenario::LoadShift {
                peak_usage_kwh: 5000.0,
                off_peak_rate: 0.05,
                peak_rate: 0.20,
                shift_pct: 1.5,
            },
            0.11,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_pool_pump_swap() {
        let result = simulate_pool_pump_swap(1.5, 8.0, 0.11);
        assert!(result.annual_savings.value() > 0.0);
        assert!(result.capex.value() > 0.0);
        assert!(result.simple_payback_years > 0.0);
        assert!(result.simple_payback_years < 10.0);
    }

    #[test]
    fn test_tou_shift() {
        let result = simulate_tou_shift(1500.0, 0.30, 0.11, 0.23, 0.065);
        // With 30% peak at $0.23 and 70% off-peak at $0.065:
        // Current: 18000 * 0.11 = $1980
        // TOU: 5400*0.23 + 12600*0.065 = 1242 + 819 = $2061
        // In this scenario TOU is actually more expensive (peak rate is very high)
        // That is a valid outcome — the simulation shows the truth.
        assert!(result.current_annual_cost.value() > 0.0);
        assert!(result.projected_annual_cost.value() > 0.0);
    }

    #[test]
    fn test_insulation_upgrade() {
        let result = simulate_insulation_upgrade(19.0, 49.0, 2000.0, 8000.0);
        assert!(result.annual_savings.value() > 0.0);
        assert!(result.capex.value() > 0.0);
        assert!(result.simple_payback_years > 0.0);
    }

    #[test]
    fn test_device_profile_annual_kwh() {
        let dp = DeviceProfile {
            name: "Test".into(),
            nameplate_watts: 1000.0,
            daily_run_hours: 10.0,
            efficiency_factor: 1.0,
        };
        // 1000W * 10h * 365d / 1000 = 3650 kWh
        assert!((dp.annual_kwh() - 3650.0).abs() < 0.01);
    }
}
