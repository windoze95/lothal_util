use axum::extract::State;
use axum::routing::get;
use axum::Router;

use lothal_ontology::ObjectUri;

use crate::charts;
use crate::error::WebError;
use crate::state::AppState;
use crate::templates::*;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(pulse))
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

/// Three actions surfaced inline on the Pulse page. Order matters — this is
/// the order they render. Each entry is `(action_name, label, description)`
/// and maps to a short inline form whose inputs are defined in the template.
const PULSE_ACTION_CARDS: &[(&str, &str, &str)] = &[
    (
        "record_observation",
        "Record observation",
        "Attach a free-text note to the site.",
    ),
    (
        "scoped_briefing",
        "Scoped briefing",
        "LLM briefing over the site's ontology slice.",
    ),
    (
        "run_diagnostic",
        "Run diagnostic",
        "Root-cause hypothesis for a circuit or device.",
    ),
];

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
                href: "/",
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
                href: "/",
            });
        }
        if let Some(ref lv) = ctx.livestock_summary {
            stats.push(StatCard {
                label: "Eggs",
                value: format!("{:.0}", lv.eggs),
                unit: "",
                trend: None,
                color: "bio",
                href: "/",
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
                href: None,
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

    // Recent events across the site (last 7 days, top 10).
    let recent_events = if let Some(sid) = site_id {
        let site_uri = ObjectUri::new("site", sid);
        let now = chrono::Utc::now();
        let start = now - chrono::Duration::days(7);
        match lothal_ontology::query::events_for(&state.pool, &[site_uri], start, now, None).await {
            Ok(events) => events
                .into_iter()
                .take(10)
                .map(|ev| {
                    // If the event has subjects, link to the first subject's entity page.
                    let href = ev
                        .subjects
                        .0
                        .first()
                        .and_then(|v| {
                            let kind = v.get("kind").and_then(|k| k.as_str())?;
                            let id = v.get("id").and_then(|i| i.as_str())?;
                            Some(format!("/e/{kind}/{id}"))
                        });
                    EventListEntry {
                        time: ev.time.format("%b %d %H:%M").to_string(),
                        kind: ev.kind,
                        summary: ev.summary,
                        severity: ev.severity,
                        href,
                    }
                })
                .collect(),
            Err(_) => Vec::new(),
        }
    } else {
        Vec::new()
    };

    // Inline action cards (only meaningful when a site exists — each form
    // posts to /e/site/{site_id}/actions/{name}).
    let action_cards = if let Some(sid) = site_id {
        let registry_actions = state.registry.list();
        PULSE_ACTION_CARDS
            .iter()
            .filter(|(name, _, _)| registry_actions.iter().any(|a| a.name() == *name))
            .map(|(name, label, desc)| PulseActionCard {
                name: (*name).to_string(),
                label: (*label).to_string(),
                description: (*desc).to_string(),
                site_id: sid.to_string(),
            })
            .collect()
    } else {
        Vec::new()
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
        recent_events,
        action_cards,
        site_href: site_id.map(|sid| format!("/e/site/{sid}")),
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
