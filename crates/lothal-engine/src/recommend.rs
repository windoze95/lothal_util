//! Recommendation generation and ranking for Oklahoma homes.
//!
//! Examines site metadata, installed devices, and billing data to produce
//! a prioritized list of efficiency recommendations.

use uuid::Uuid;

use lothal_core::ontology::bill::Bill;
use lothal_core::ontology::device::{Device, DeviceKind};
use lothal_core::ontology::experiment::{HypothesisCategory, Recommendation};
use lothal_core::ontology::livestock::Flock;
use lothal_core::ontology::tree::Tree;
use lothal_core::ontology::water::{Pool, SepticSystem, WaterSource};
use lothal_core::Usd;

use crate::baseline::BaselineModel;

// ---------------------------------------------------------------------------
// Site context
// ---------------------------------------------------------------------------

/// Everything the recommender needs to know about a site.
#[derive(Debug, Clone)]
pub struct SiteContext {
    pub site_id: Uuid,
    /// Year the primary structure was built.
    pub year_built: Option<i32>,
    /// Whether the property has a swimming pool.
    pub has_pool: bool,
    /// IECC climate zone (e.g. "3A" for Oklahoma).
    pub climate_zone: Option<String>,
    /// All known devices at the site.
    pub devices: Vec<Device>,
    /// Recent utility bills (used for usage estimates).
    pub recent_bills: Vec<Bill>,
    /// A cooling or heating baseline model, if one has been computed.
    pub baseline: Option<BaselineModel>,
    // --- Property operations context ---
    /// Significant trees on the property.
    pub trees: Vec<Tree>,
    /// Pools on the property (first-class entities).
    pub pools: Vec<Pool>,
    /// Water sources available.
    pub water_sources: Vec<WaterSource>,
    /// Septic system, if any.
    pub septic: Option<SepticSystem>,
    /// Livestock flocks on the property.
    pub flocks: Vec<Flock>,
}

// ---------------------------------------------------------------------------
// Generation
// ---------------------------------------------------------------------------

/// Examine the site context and emit every applicable recommendation.
pub fn generate_recommendations(ctx: &SiteContext) -> Vec<Recommendation> {
    let mut recs = Vec::new();

    recommend_pool_pump(ctx, &mut recs);
    recommend_tou_rate(ctx, &mut recs);
    recommend_air_sealing(ctx, &mut recs);
    recommend_attic_insulation(ctx, &mut recs);
    recommend_hpwh(ctx, &mut recs);
    recommend_smart_thermostat(ctx, &mut recs);
    recommend_led_lighting(ctx, &mut recs);
    // Property operations recommendations
    recommend_pool_cover(ctx, &mut recs);
    recommend_rainwater_capture(ctx, &mut recs);
    recommend_tree_shade(ctx, &mut recs);
    recommend_septic_maintenance(ctx, &mut recs);
    recommend_chicken_efficiency(ctx, &mut recs);
    recommend_garden_drip(ctx, &mut recs);

    rank_recommendations(&mut recs);
    recs
}

// ---------------------------------------------------------------------------
// Ranking
// ---------------------------------------------------------------------------

/// Sort recommendations by priority score descending.
///
/// `priority_score = (annual_savings / payback_years) * confidence`
pub fn rank_recommendations(recs: &mut Vec<Recommendation>) {
    for rec in recs.iter_mut() {
        let payback = if rec.payback_years > 0.0 && rec.payback_years.is_finite() {
            rec.payback_years
        } else {
            // Treat zero-capex items as 0.5-year payback for ranking purposes
            // so they sort high but below items with real savings.
            0.5
        };
        rec.priority_score = (rec.estimated_annual_savings.value() / payback) * rec.confidence;
    }
    recs.sort_by(|a, b| {
        b.priority_score
            .partial_cmp(&a.priority_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
}

// ---------------------------------------------------------------------------
// Individual recommendation templates
// ---------------------------------------------------------------------------

fn has_device_kind(devices: &[Device], kind: DeviceKind) -> bool {
    devices.iter().any(|d| d.kind == kind)
}

fn has_variable_speed_pool_pump(devices: &[Device]) -> bool {
    devices.iter().any(|d| {
        d.kind == DeviceKind::PoolPump
            && d.name.to_lowercase().contains("variable")
    })
}

/// 1. Pool pump upgrade — recommend if pool exists but no variable-speed pump.
fn recommend_pool_pump(ctx: &SiteContext, recs: &mut Vec<Recommendation>) {
    if !ctx.has_pool {
        return;
    }
    if has_variable_speed_pool_pump(&ctx.devices) {
        return;
    }

    let mut rec = Recommendation::new(
        ctx.site_id,
        "Upgrade to variable-speed pool pump".to_string(),
        "Replace single-speed pool pump with a Pentair IntelliFlo3 or similar \
         variable-speed pump. Variable-speed pumps run longer at lower RPM, \
         delivering the same turnover at 70-80% less energy. Typical Oklahoma \
         savings: $400-700/year."
            .to_string(),
        HypothesisCategory::DeviceSwap,
        Usd::new(550.0),  // midpoint estimate
        Usd::new(1500.0), // typical installed
    );
    rec.confidence = 0.8;
    recs.push(rec);
}

/// 2. TOU rate switch — recommend if on a flat rate.
fn recommend_tou_rate(ctx: &SiteContext, recs: &mut Vec<Recommendation>) {
    // Heuristic: if we have bills, check if any indication of TOU.
    // Without detailed rate metadata we always suggest evaluating TOU.
    if ctx.recent_bills.is_empty() {
        return;
    }

    let avg_monthly: f64 = if ctx.recent_bills.is_empty() {
        0.0
    } else {
        ctx.recent_bills.iter().map(|b| b.total_usage).sum::<f64>()
            / ctx.recent_bills.len() as f64
    };

    let est_savings = avg_monthly * 12.0 * 0.05; // rough 5% savings estimate

    let mut rec = Recommendation::new(
        ctx.site_id,
        "Evaluate time-of-use rate (OG&E SmartHours)".to_string(),
        format!(
            "Based on ~{avg_monthly:.0} kWh/month average usage, switching to \
             OG&E SmartHours may save money if you can shift load away from \
             the 2-7 PM peak window (June-September weekdays). Run the TOU \
             simulation with your actual peak/off-peak split for an accurate estimate."
        ),
        HypothesisCategory::RateOptimization,
        Usd::new(est_savings),
        Usd::zero(),
    );
    rec.confidence = 0.5; // depends heavily on load profile
    rec.data_requirements =
        Some("Interval meter data needed for accurate peak/off-peak split".to_string());
    recs.push(rec);
}

/// 3. Air sealing — recommend for pre-2000 homes.
fn recommend_air_sealing(ctx: &SiteContext, recs: &mut Vec<Recommendation>) {
    let year = match ctx.year_built {
        Some(y) if y < 2000 => y,
        _ => return,
    };

    let hvac_cost = estimate_annual_hvac_cost(ctx);
    let est_savings = hvac_cost * 0.15; // 10-20%, use midpoint

    let mut rec = Recommendation::new(
        ctx.site_id,
        "Professional air sealing".to_string(),
        format!(
            "Home built in {year} likely has significant air leakage around \
             penetrations, sill plates, and recessed lighting. Professional \
             air sealing typically reduces HVAC costs by 10-20%. A blower-door \
             test can quantify the opportunity."
        ),
        HypothesisCategory::EnvelopeUpgrade,
        Usd::new(est_savings),
        Usd::new(1000.0), // $500-$1500 midpoint
    );
    rec.confidence = 0.6;
    recs.push(rec);
}

/// 4. Attic insulation — recommend for pre-2000 homes (likely under R-30).
fn recommend_attic_insulation(ctx: &SiteContext, recs: &mut Vec<Recommendation>) {
    let year = match ctx.year_built {
        Some(y) if y < 2000 => y,
        _ => return,
    };

    let hvac_cost = estimate_annual_hvac_cost(ctx);
    let est_savings = hvac_cost * 0.125; // 10-15%, use midpoint

    let mut rec = Recommendation::new(
        ctx.site_id,
        "Upgrade attic insulation to R-49".to_string(),
        format!(
            "Home built in {year} likely has R-19 to R-30 attic insulation. \
             Current IECC code for Oklahoma (Zone 3A) recommends R-49. \
             Adding blown-in insulation typically reduces HVAC costs by 10-15%."
        ),
        HypothesisCategory::EnvelopeUpgrade,
        Usd::new(est_savings),
        Usd::new(2500.0), // ~2000 sqft * $1.25/sqft
    );
    rec.confidence = 0.65;
    recs.push(rec);
}

/// 5. Heat pump water heater — recommend if electric tank water heater found.
fn recommend_hpwh(ctx: &SiteContext, recs: &mut Vec<Recommendation>) {
    if !has_device_kind(&ctx.devices, DeviceKind::WaterHeater) {
        return;
    }

    let mut rec = Recommendation::new(
        ctx.site_id,
        "Heat pump water heater (HPWH)".to_string(),
        "Replace standard electric resistance water heater with a heat pump \
         water heater (e.g. Rheem ProTerra). HPWHs use 2-3x less energy by \
         moving heat from ambient air. Typical savings: $200-400/year. \
         Federal tax credit and utility rebates may apply."
            .to_string(),
        HypothesisCategory::DeviceSwap,
        Usd::new(300.0),  // midpoint
        Usd::new(2000.0), // $1500-2500 midpoint
    );
    rec.confidence = 0.7;
    recs.push(rec);
}

/// 6. Smart thermostat — recommend if no smart thermostat device found.
fn recommend_smart_thermostat(ctx: &SiteContext, recs: &mut Vec<Recommendation>) {
    let has_smart = ctx.devices.iter().any(|d| {
        d.kind == DeviceKind::Thermostat
            && d.name.to_lowercase().contains("smart")
    });
    if has_smart {
        return;
    }

    let hvac_cost = estimate_annual_hvac_cost(ctx);
    let est_savings = hvac_cost * 0.125; // 10-15% midpoint

    let mut rec = Recommendation::new(
        ctx.site_id,
        "Install smart thermostat".to_string(),
        "A smart thermostat (Ecobee, Nest, etc.) can reduce HVAC costs by \
         10-15% through occupancy sensing, scheduling, and learning. \
         Many utilities offer rebates. Pairs well with TOU rate optimization."
            .to_string(),
        HypothesisCategory::BehaviorChange,
        Usd::new(est_savings),
        Usd::new(200.0), // $150-250 midpoint
    );
    rec.confidence = 0.7;
    recs.push(rec);
}

/// 7. LED lighting — universal fallback.
fn recommend_led_lighting(ctx: &SiteContext, recs: &mut Vec<Recommendation>) {
    let mut rec = Recommendation::new(
        ctx.site_id,
        "Switch remaining bulbs to LED".to_string(),
        "LED bulbs use ~75% less energy than incandescent and last 15-25x \
         longer. Even if most fixtures have been converted, check garage, \
         outdoor, and closet fixtures for remaining incandescent or CFL bulbs."
            .to_string(),
        HypothesisCategory::DeviceSwap,
        Usd::new(50.0),
        Usd::new(50.0),
    );
    rec.confidence = 0.5; // universal, low confidence on exact savings
    recs.push(rec);
}

// ---------------------------------------------------------------------------
// Property operations recommendation templates
// ---------------------------------------------------------------------------

/// 8. Pool cover — recommend if pool exists without a cover.
fn recommend_pool_cover(ctx: &SiteContext, recs: &mut Vec<Recommendation>) {
    let uncovered_pool = ctx.pools.iter().find(|p| p.cover_type.is_none());
    let pool = match uncovered_pool {
        Some(p) => p,
        None => {
            // Fall back to has_pool boolean if no Pool entity exists
            if !ctx.has_pool {
                return;
            }
            // No Pool entity but has_pool is true — still recommend
            let mut rec = Recommendation::new(
                ctx.site_id,
                "Add a pool cover".to_string(),
                "A solar or manual pool cover reduces evaporation by 50-70%, \
                 cutting water loss, chemical consumption, and heating costs. \
                 Typical savings: $200-500/year depending on pool size."
                    .to_string(),
                HypothesisCategory::WaterConservation,
                Usd::new(350.0),
                Usd::new(200.0),
            );
            rec.confidence = 0.7;
            recs.push(rec);
            return;
        }
    };

    let surface = pool.surface_area_sqft.map(|s| s.value()).unwrap_or(400.0);
    // Rule of thumb: uncovered pool loses ~0.25 inches/day in Oklahoma summer.
    // 1 inch over 400 sqft = ~250 gallons.
    let est_annual_savings = (surface / 400.0) * 350.0;

    let mut rec = Recommendation::new(
        ctx.site_id,
        "Add a pool cover".to_string(),
        format!(
            "Pool '{}' ({:.0} gal) has no cover. A solar or manual cover \
             reduces evaporation 50-70%, saving water, chemicals, and heat. \
             Estimated savings: ${est_annual_savings:.0}/year.",
            pool.name,
            pool.volume_gallons.value()
        ),
        HypothesisCategory::WaterConservation,
        Usd::new(est_annual_savings),
        Usd::new(200.0),
    );
    rec.confidence = 0.7;
    recs.push(rec);
}

/// 9. Rainwater capture — recommend if no cistern/rainwater source exists.
fn recommend_rainwater_capture(ctx: &SiteContext, recs: &mut Vec<Recommendation>) {
    let has_capture = ctx.water_sources.iter().any(|ws| {
        matches!(
            ws.kind,
            lothal_core::ontology::water::WaterSourceKind::Cistern
                | lothal_core::ontology::water::WaterSourceKind::RainwaterCollection
        )
    });
    if has_capture {
        return;
    }

    // Oklahoma averages ~36 inches of rain per year.
    // A typical 2000 sqft roof captures: 2000 * 36 * 0.623 = ~44,856 gallons/year.
    let roof_sqft = 2000.0; // conservative default
    let annual_rain_in = 36.0;
    let capturable_gallons = roof_sqft * annual_rain_in * 0.623;

    let mut rec = Recommendation::new(
        ctx.site_id,
        "Install rainwater collection".to_string(),
        format!(
            "No rainwater capture detected. A ~{roof_sqft:.0} sqft roof at \
             {annual_rain_in:.0}\"/year could yield ~{capturable_gallons:.0} gallons \
             annually. A 500-gallon rain barrel system ($200-400) offsets \
             irrigation and chicken water needs."
        ),
        HypothesisCategory::WaterConservation,
        Usd::new(150.0), // conservative: partial offset of irrigation
        Usd::new(300.0),
    );
    rec.confidence = 0.5;
    rec.data_requirements = Some("Actual roof area and municipal water cost needed for precise ROI".to_string());
    recs.push(rec);
}

/// 10. Tree shade — recommend planting if south/west walls lack coverage.
fn recommend_tree_shade(ctx: &SiteContext, recs: &mut Vec<Recommendation>) {
    // Only recommend if we have tree data and see a gap
    let has_sw_shade = ctx.trees.iter().any(|t| {
        t.shade_direction
            .as_ref()
            .map(|d| {
                let d = d.to_uppercase();
                d.contains('S') || d.contains('W')
            })
            .unwrap_or(false)
    });
    if has_sw_shade {
        return;
    }

    let mut rec = Recommendation::new(
        ctx.site_id,
        "Plant shade tree on south/west exposure".to_string(),
        "No shade tree detected on the south or west side of the property. \
         A well-placed deciduous tree can reduce cooling costs by 15-35% \
         by shading walls and roof during summer while allowing winter sun. \
         Savings compound as the tree matures."
            .to_string(),
        HypothesisCategory::LandManagement,
        Usd::new(200.0), // conservative: grows over years
        Usd::new(150.0), // tree + planting
    );
    rec.confidence = 0.5;
    recs.push(rec);
}

/// 11. Septic maintenance — recommend proactive scheduling if overdue.
fn recommend_septic_maintenance(ctx: &SiteContext, recs: &mut Vec<Recommendation>) {
    let septic = match &ctx.septic {
        Some(s) => s,
        None => return,
    };

    let days = match septic.days_until_pump() {
        Some(d) => d,
        None => return, // not enough data
    };

    if days > 180 {
        return; // more than 6 months out, not urgent
    }

    let urgency = if days < 0 { "overdue" } else { "approaching" };

    let mut rec = Recommendation::new(
        ctx.site_id,
        "Schedule septic pump-out".to_string(),
        format!(
            "Septic pump-out is {urgency} ({} days). Regular pumping ($300-500) \
             prevents leach field failure ($10,000+). Consider spacing heavy \
             water use days to reduce daily load.",
            days.abs()
        ),
        HypothesisCategory::Maintenance,
        Usd::new(0.0),   // preventive, not savings per se
        Usd::new(400.0),
    );
    rec.confidence = 0.8;
    recs.push(rec);
}

/// 12. Chicken coop efficiency — recommend if flock exists.
fn recommend_chicken_efficiency(ctx: &SiteContext, recs: &mut Vec<Recommendation>) {
    if ctx.flocks.is_empty() {
        return;
    }

    let has_auto_door = has_device_kind(&ctx.devices, DeviceKind::CoopDoor);
    if has_auto_door {
        return;
    }

    let mut rec = Recommendation::new(
        ctx.site_id,
        "Install automatic coop door".to_string(),
        "An automatic coop door ($100-200) opens at dawn and closes at dusk, \
         reducing predator risk and eliminating a daily chore. Solar-powered \
         models need no wiring. Pairs well with a coop camera for remote \
         monitoring."
            .to_string(),
        HypothesisCategory::LivestockOptimization,
        Usd::new(50.0),  // predator loss prevention + time value
        Usd::new(150.0),
    );
    rec.confidence = 0.7;
    recs.push(rec);
}

/// 13. Garden drip irrigation — recommend if no irrigation controller.
fn recommend_garden_drip(ctx: &SiteContext, recs: &mut Vec<Recommendation>) {
    let has_irrigation = has_device_kind(&ctx.devices, DeviceKind::IrrigationController);
    if has_irrigation {
        return;
    }

    // Only recommend if there's some indication of gardening
    let has_sprinkler = has_device_kind(&ctx.devices, DeviceKind::Sprinkler);
    if !has_sprinkler {
        return;
    }

    let mut rec = Recommendation::new(
        ctx.site_id,
        "Switch to drip irrigation with smart controller".to_string(),
        "Drip irrigation uses 30-50% less water than sprinklers by \
         delivering water directly to root zones. A smart controller ($50-150) \
         adjusts watering based on weather, reducing waste further."
            .to_string(),
        HypothesisCategory::WaterConservation,
        Usd::new(100.0),
        Usd::new(200.0),
    );
    rec.confidence = 0.6;
    recs.push(rec);
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Rough estimate of annual HVAC cost from bills.  Falls back to a typical
/// Oklahoma home if no bills are available.
fn estimate_annual_hvac_cost(ctx: &SiteContext) -> f64 {
    if ctx.recent_bills.is_empty() {
        // Typical Oklahoma home: ~$2,000/yr electric, ~60% HVAC
        return 1200.0;
    }

    let annual_cost: f64 = ctx
        .recent_bills
        .iter()
        .map(|b| b.total_amount.value())
        .sum::<f64>()
        * (12.0 / ctx.recent_bills.len() as f64);

    // HVAC is typically 50-70% of an Oklahoma electric bill.
    annual_cost * 0.60
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use lothal_core::ontology::device::Device;

    fn test_site(year: Option<i32>, has_pool: bool) -> SiteContext {
        SiteContext {
            site_id: Uuid::new_v4(),
            year_built: year,
            has_pool,
            climate_zone: Some("3A".to_string()),
            devices: Vec::new(),
            recent_bills: Vec::new(),
            baseline: None,
            trees: Vec::new(),
            pools: Vec::new(),
            water_sources: Vec::new(),
            septic: None,
            flocks: Vec::new(),
        }
    }

    #[test]
    fn test_pool_recommendation_when_pool_present() {
        let ctx = test_site(Some(1995), true);
        let recs = generate_recommendations(&ctx);
        assert!(recs.iter().any(|r| r.title.contains("pool pump")));
    }

    #[test]
    fn test_no_pool_recommendation_when_no_pool() {
        let ctx = test_site(Some(1995), false);
        let recs = generate_recommendations(&ctx);
        assert!(!recs.iter().any(|r| r.title.contains("pool pump")));
    }

    #[test]
    fn test_no_pool_rec_if_variable_speed_present() {
        let mut ctx = test_site(Some(1995), true);
        let structure_id = Uuid::new_v4();
        let mut pump = Device::new(structure_id, "Variable Speed Pool Pump".to_string(), DeviceKind::PoolPump);
        pump.make = Some("Pentair".to_string());
        ctx.devices.push(pump);

        let recs = generate_recommendations(&ctx);
        assert!(!recs.iter().any(|r| r.title.contains("pool pump")));
    }

    #[test]
    fn test_envelope_recs_for_old_home() {
        let ctx = test_site(Some(1985), false);
        let recs = generate_recommendations(&ctx);
        assert!(recs.iter().any(|r| r.title.contains("air sealing")));
        assert!(recs.iter().any(|r| r.title.contains("insulation")));
    }

    #[test]
    fn test_no_envelope_recs_for_new_home() {
        let ctx = test_site(Some(2020), false);
        let recs = generate_recommendations(&ctx);
        assert!(!recs.iter().any(|r| r.title.contains("air sealing")));
        assert!(!recs.iter().any(|r| r.title.contains("insulation")));
    }

    #[test]
    fn test_hpwh_when_water_heater_present() {
        let mut ctx = test_site(Some(2010), false);
        let structure_id = Uuid::new_v4();
        ctx.devices.push(Device::new(
            structure_id,
            "Electric Water Heater".to_string(),
            DeviceKind::WaterHeater,
        ));
        let recs = generate_recommendations(&ctx);
        assert!(recs.iter().any(|r| r.title.contains("Heat pump water heater")));
    }

    #[test]
    fn test_smart_thermostat_when_none_present() {
        let ctx = test_site(Some(2010), false);
        let recs = generate_recommendations(&ctx);
        assert!(recs.iter().any(|r| r.title.contains("smart thermostat")));
    }

    #[test]
    fn test_no_smart_thermostat_when_present() {
        let mut ctx = test_site(Some(2010), false);
        let structure_id = Uuid::new_v4();
        ctx.devices.push(Device::new(
            structure_id,
            "Ecobee Smart Thermostat".to_string(),
            DeviceKind::Thermostat,
        ));
        let recs = generate_recommendations(&ctx);
        assert!(!recs.iter().any(|r| r.title.contains("smart thermostat")));
    }

    #[test]
    fn test_led_always_present() {
        let ctx = test_site(Some(2022), false);
        let recs = generate_recommendations(&ctx);
        assert!(recs.iter().any(|r| r.title.contains("LED")));
    }

    #[test]
    fn test_ranking_order() {
        let ctx = test_site(Some(1990), true);
        let recs = generate_recommendations(&ctx);
        // Verify descending priority_score
        for window in recs.windows(2) {
            assert!(
                window[0].priority_score >= window[1].priority_score,
                "Recs should be sorted by priority_score descending: {} >= {}",
                window[0].priority_score,
                window[1].priority_score
            );
        }
    }
}
