use axum::extract::State;
use axum::routing::get;
use axum::Router;
use chrono::NaiveDate;

use crate::charts;
use crate::error::WebError;
use crate::state::AppState;
use crate::templates::*;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(pulse))
        .route("/energy", get(energy))
        .route("/water", get(water))
        .route("/property", get(property))
        .route("/land", get(land))
        .route("/lab", get(lab))
        .route("/bills", get(bills))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Get the first site (single-user, single-site deployment).
async fn first_site(pool: &sqlx::PgPool) -> Result<Option<lothal_core::ontology::site::Site>, WebError> {
    let sites = lothal_db::site::list_sites(pool).await?;
    Ok(sites.into_iter().next())
}

fn site_name(site: &Option<lothal_core::ontology::site::Site>) -> String {
    site.as_ref()
        .map(|s| s.address.clone())
        .unwrap_or_else(|| "My Property".into())
}

/// Gather all bills across all accounts for a site.
async fn all_bills_for_site(
    pool: &sqlx::PgPool,
    site_id: uuid::Uuid,
    limit: usize,
) -> Vec<lothal_core::ontology::bill::Bill> {
    let accounts = lothal_db::bill::list_utility_accounts_by_site(pool, site_id)
        .await
        .unwrap_or_default();
    let mut all_bills = Vec::new();
    for acct in &accounts {
        if let Ok(bills) = lothal_db::bill::list_bills_by_account(pool, acct.id).await {
            all_bills.extend(bills);
        }
    }
    // Sort by statement date descending, take limit.
    all_bills.sort_by(|a, b| b.statement_date.cmp(&a.statement_date));
    all_bills.truncate(limit);
    all_bills
}

// ---------------------------------------------------------------------------
// Pulse (Home)
// ---------------------------------------------------------------------------

async fn pulse(State(state): State<AppState>) -> Result<PulsePage, WebError> {
    let site = first_site(&state.pool).await?;
    let name = site_name(&site);
    let site_id = site.as_ref().map(|s| s.id);

    let today = chrono::Local::now().date_naive();
    let yesterday = today - chrono::Duration::days(1);

    // Fetch latest briefing and context.
    let (briefing_text, ctx) = if let Some(sid) = site_id {
        let briefing = lothal_db::ai::get_briefing(&state.pool, sid, yesterday)
            .await
            .ok()
            .flatten();
        let ctx = lothal_ai::briefing::context::gather_context(&state.pool, sid, yesterday)
            .await
            .ok();
        (briefing.map(|b| b.content), ctx)
    } else {
        (None, None)
    };

    // Build stat cards.
    let mut stats = Vec::new();
    if let Some(ref ctx) = ctx {
        if let Some(kwh) = ctx.total_kwh {
            stats.push(StatCard {
                label: "Energy",
                value: format!("{kwh:.1}"),
                unit: "kWh",
                trend: ctx.baseline_comparison.as_ref().map(|b| b.deviation_pct),
                color: "energy",
                href: "/energy",
            });
        }
        if let Some(cost) = ctx.estimated_cost {
            stats.push(StatCard {
                label: "Cost",
                value: format!("${cost:.2}"),
                unit: "",
                trend: None,
                color: "energy",
                href: "/bills",
            });
        }
        if let Some(ref w) = ctx.weather {
            stats.push(StatCard {
                label: "Weather",
                value: format!("{:.0}/{:.0}", w.max_temp_f, w.min_temp_f),
                unit: "F",
                trend: None,
                color: "heat",
                href: "/energy",
            });
        }
        if let Some(ref lv) = ctx.livestock_summary {
            stats.push(StatCard {
                label: "Eggs",
                value: format!("{:.0}", lv.eggs),
                unit: "",
                trend: None,
                color: "bio",
                href: "/land",
            });
        }
    }

    // Build alerts.
    let mut alerts = Vec::new();
    if let Some(ref ctx) = ctx {
        for m in &ctx.maintenance_due {
            alerts.push(Alert {
                message: format!("{} \u{2014} due {}", m.description, m.due_date),
                severity: AlertSeverity::Warning,
                href: None,
            });
        }
        if let Some(ref sep) = ctx.septic_alert {
            let msg = if sep.is_overdue {
                format!("Septic pump-out OVERDUE by {} days", sep.days_until_pump.abs())
            } else {
                format!("Septic pump-out due in {} days", sep.days_until_pump)
            };
            alerts.push(Alert {
                message: msg,
                severity: if sep.is_overdue { AlertSeverity::Danger } else { AlertSeverity::Warning },
                href: Some("/water".into()),
            });
        }
    }

    // Experiments.
    let experiments: Vec<ExperimentSummary> = ctx
        .as_ref()
        .map(|c| {
            c.active_experiments
                .iter()
                .map(|e| ExperimentSummary {
                    title: e.title.clone(),
                    status: "Active".into(),
                    days_active: 0,
                })
                .collect()
        })
        .unwrap_or_default();

    // Top recommendation.
    let top_recommendation = if let Some(sid) = site_id {
        build_top_recommendation(&state.pool, sid).await.ok().flatten()
    } else {
        None
    };

    Ok(PulsePage {
        active_page: "pulse".into(),
        site_name: name,
        briefing: briefing_text,
        briefing_date: yesterday.format("%B %d, %Y").to_string(),
        stats,
        alerts,
        experiments,
        top_recommendation,
    })
}

async fn build_top_recommendation(
    pool: &sqlx::PgPool,
    site_id: uuid::Uuid,
) -> Result<Option<RecommendationSummary>, WebError> {
    let recs = build_recommendations(pool, site_id).await?;
    Ok(recs.into_iter().next())
}

pub async fn build_recommendations(
    pool: &sqlx::PgPool,
    site_id: uuid::Uuid,
) -> Result<Vec<RecommendationSummary>, WebError> {
    let structures = lothal_db::site::get_structures_by_site(pool, site_id).await?;
    let mut devices = Vec::new();
    let mut year_built = None;
    let mut has_pool = false;
    for s in &structures {
        let devs = lothal_db::device::list_devices_by_structure(pool, s.id).await?;
        devices.extend(devs);
        if s.year_built.is_some() && year_built.is_none() {
            year_built = s.year_built;
        }
        if s.has_pool {
            has_pool = true;
        }
    }

    let site = lothal_db::site::get_site(pool, site_id).await?.unwrap();
    let bills = all_bills_for_site(pool, site_id, 12).await;
    let pools = lothal_db::water::list_pools_by_site(pool, site_id).await.unwrap_or_default();
    let water_sources = lothal_db::water::list_water_sources_by_site(pool, site_id).await.unwrap_or_default();
    let septic = lothal_db::water::get_septic_system(pool, site_id).await.unwrap_or(None);
    let flocks = lothal_db::livestock::list_flocks_by_site(pool, site_id).await.unwrap_or_default();

    let ctx = lothal_engine::recommend::SiteContext {
        site_id,
        year_built,
        has_pool,
        climate_zone: site.climate_zone.clone(),
        devices,
        recent_bills: bills,
        baseline: None,
        pools,
        water_sources,
        septic,
        flocks,
    };
    let recs = lothal_engine::recommend::generate_recommendations(&ctx);
    Ok(recs
        .into_iter()
        .map(|r| RecommendationSummary {
            title: r.title,
            category: format!("{:?}", r.category),
            annual_savings: r.estimated_annual_savings.value(),
            payback_years: r.payback_years,
            confidence: r.confidence,
            description: r.description,
        })
        .collect())
}

// ---------------------------------------------------------------------------
// Energy
// ---------------------------------------------------------------------------

async fn energy(State(state): State<AppState>) -> Result<EnergyPage, WebError> {
    let site = first_site(&state.pool).await?;
    let name = site_name(&site);

    let yesterday = chrono::Local::now().date_naive() - chrono::Duration::days(1);

    let (total_kwh, circuits) = if let Some(ref s) = site {
        let ctx = lothal_ai::briefing::context::gather_context(&state.pool, s.id, yesterday)
            .await
            .ok();
        let kwh = ctx.as_ref().and_then(|c| c.total_kwh).unwrap_or(0.0);
        let circs: Vec<CircuitSummary> = ctx
            .as_ref()
            .map(|c| {
                c.circuit_anomalies
                    .iter()
                    .map(|a| CircuitSummary {
                        label: a.circuit_label.clone(),
                        kwh_today: a.actual_hours,
                        pct_of_total: if kwh > 0.0 { (a.actual_hours / kwh) * 100.0 } else { 0.0 },
                    })
                    .collect()
            })
            .unwrap_or_default();
        (kwh, circs)
    } else {
        (0.0, Vec::new())
    };

    let usage_chart = charts::to_chart_json(&charts::energy_usage_chart(vec![], vec![], vec![]));
    let circuit_labels: Vec<String> = circuits.iter().map(|c| c.label.clone()).collect();
    let circuit_values: Vec<f64> = circuits.iter().map(|c| c.kwh_today).collect();
    let circuit_chart = charts::to_chart_json(&charts::circuit_breakdown_chart(circuit_labels, circuit_values));

    Ok(EnergyPage {
        active_page: "energy".into(),
        site_name: name,
        total_kwh_today: total_kwh,
        estimated_cost_today: total_kwh * 0.11,
        circuits,
        usage_chart,
        circuit_chart,
        baseline_r_squared: None,
        baseline_slope: None,
        baseline_intercept: None,
    })
}

// ---------------------------------------------------------------------------
// Water
// ---------------------------------------------------------------------------

async fn water(State(state): State<AppState>) -> Result<WaterPage, WebError> {
    let site = first_site(&state.pool).await?;
    let name = site_name(&site);

    let (pools, septic) = if let Some(ref s) = site {
        let p: Vec<PoolDisplay> = lothal_db::water::list_pools_by_site(&state.pool, s.id)
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|pool| PoolDisplay {
                name: pool.name,
                volume_gallons: pool.volume_gallons.value(),
                pump_runtime_hours: None,
                last_chlorine_ppm: None,
                last_ph: None,
                last_temp_f: None,
            })
            .collect();
        let sep = lothal_db::water::get_septic_system(&state.pool, s.id)
            .await
            .unwrap_or(None)
            .map(|sep| SepticDisplay {
                tank_capacity_gallons: sep.tank_capacity_gallons.map(|g| g.value()).unwrap_or(0.0),
                days_until_pump: sep.days_until_pump().unwrap_or(999),
                is_overdue: sep.days_until_pump().is_some_and(|d| d < 0),
                daily_load_estimate: sep.daily_load_estimate_gallons,
            });
        (p, sep)
    } else {
        (Vec::new(), None)
    };

    let has_water_data = !pools.is_empty() || septic.is_some();

    Ok(WaterPage {
        active_page: "water".into(),
        site_name: name,
        pools,
        septic,
        has_water_data,
    })
}

// ---------------------------------------------------------------------------
// Property
// ---------------------------------------------------------------------------

async fn property(State(state): State<AppState>) -> Result<PropertyPage, WebError> {
    let site = first_site(&state.pool).await?;
    let name = site_name(&site);

    let zones = if let Some(ref s) = site {
        lothal_db::property_zone::list_property_zones_by_site(&state.pool, s.id)
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|z| ZoneDisplay {
                id: z.id.to_string(),
                name: z.name.clone(),
                kind: format!("{:?}", z.kind),
                area_sqft: z.area_sqft.map(|a| a.value()),
            })
            .collect()
    } else {
        Vec::new()
    };

    Ok(PropertyPage {
        active_page: "property".into(),
        site_name: name,
        zones,
    })
}

// ---------------------------------------------------------------------------
// Land
// ---------------------------------------------------------------------------

async fn land(State(state): State<AppState>) -> Result<LandPage, WebError> {
    let site = first_site(&state.pool).await?;
    let name = site_name(&site);
    let today = chrono::Local::now().date_naive();

    let (flocks, garden_beds) = if let Some(ref s) = site {
        let fl = lothal_db::livestock::list_flocks_by_site(&state.pool, s.id)
            .await
            .unwrap_or_default();
        let mut flock_displays = Vec::new();
        for f in &fl {
            let summary = lothal_db::livestock::get_flock_daily_summary(&state.pool, f.id, today)
                .await
                .unwrap_or_default();
            flock_displays.push(FlockDisplay {
                name: f.name.clone(),
                breed: f.breed.clone(),
                bird_count: f.bird_count,
                eggs_today: summary.eggs,
                feed_today_lbs: summary.feed_lbs,
                status: format!("{:?}", f.status),
            });
        }

        let gb: Vec<GardenBedDisplay> = lothal_db::garden::list_garden_beds_by_site(&state.pool, s.id)
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|b| GardenBedDisplay {
                name: b.name.clone(),
                bed_type: format!("{:?}", b.bed_type),
                area_sqft: b.area_sqft.map(|a| a.value()),
                active_plantings: 0,
            })
            .collect();
        (flock_displays, gb)
    } else {
        (Vec::new(), Vec::new())
    };

    let has_livestock = !flocks.is_empty();
    let has_garden = !garden_beds.is_empty();

    Ok(LandPage {
        active_page: "land".into(),
        site_name: name,
        flocks,
        garden_beds,
        has_livestock,
        has_garden,
    })
}

// ---------------------------------------------------------------------------
// Lab
// ---------------------------------------------------------------------------

async fn lab(State(state): State<AppState>) -> Result<LabPage, WebError> {
    let site = first_site(&state.pool).await?;
    let name = site_name(&site);
    let site_id = site.as_ref().map(|s| s.id);

    let recommendations = if let Some(sid) = site_id {
        build_recommendations(&state.pool, sid).await.unwrap_or_default()
    } else {
        Vec::new()
    };

    let experiments: Vec<ExperimentSummary> = if let Some(sid) = site_id {
        lothal_db::experiment::list_experiments_by_site(&state.pool, sid)
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|e| ExperimentSummary {
                title: format!("Experiment {}", e.id.to_string().get(..8).unwrap_or("?")),
                status: format!("{:?}", e.status),
                days_active: 0,
            })
            .collect()
    } else {
        Vec::new()
    };

    Ok(LabPage {
        active_page: "lab".into(),
        site_name: name,
        recommendations,
        experiments,
    })
}

// ---------------------------------------------------------------------------
// Bills
// ---------------------------------------------------------------------------

async fn bills(State(state): State<AppState>) -> Result<BillsPage, WebError> {
    let site = first_site(&state.pool).await?;
    let name = site_name(&site);

    let bill_list: Vec<BillSummary> = if let Some(ref s) = site {
        all_bills_for_site(&state.pool, s.id, 24)
            .await
            .into_iter()
            .map(|b| BillSummary {
                id: b.id.to_string(),
                account_name: String::new(),
                utility_type: format!("{:?}", b.usage_unit),
                period: format!("{} \u{2014} {}", b.period.range.start, b.period.range.end),
                usage: b.total_usage,
                usage_unit: format!("{:?}", b.usage_unit),
                amount: b.total_amount.value(),
                daily_rate: b.daily_cost().map(|c: lothal_core::Usd| c.value()).unwrap_or(0.0),
            })
            .collect()
    } else {
        Vec::new()
    };

    let bills_chart = charts::to_chart_json(&charts::bills_stacked_chart(vec![], vec![], vec![], vec![]));

    Ok(BillsPage {
        active_page: "bills".into(),
        site_name: name,
        bills: bill_list,
        bills_chart,
    })
}

