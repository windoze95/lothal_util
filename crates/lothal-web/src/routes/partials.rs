use std::collections::BTreeMap;

use axum::extract::{Query, State};
use axum::response::Html;
use axum::routing::{get, post};
use axum::Router;
use chrono::{Datelike, Local, NaiveDate};
use serde::Deserialize;
use sqlx::PgPool;
use uuid::Uuid;

use lothal_ai::provider::{CompletionRequest, LlmClient, Message, Role};
use lothal_core::ontology::utility::UtilityType;

use crate::charts;
use crate::error::WebError;
use crate::state::AppState;
use crate::templates::*;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/partials/energy/chart", get(energy_chart_partial))
        .route("/partials/energy/circuits", get(circuits_partial))
        .route("/partials/bills/chart", get(bills_chart_partial))
        .route("/partials/lab/simulate", post(simulate_partial))
        .route("/partials/chat/send", post(chat_send_partial))
}

#[derive(Deserialize)]
pub struct ChartRangeQuery {
    pub range: Option<String>,
}

async fn energy_chart_partial(
    State(state): State<AppState>,
    Query(query): Query<ChartRangeQuery>,
) -> Result<ChartPartial, WebError> {
    let range = query.range.unwrap_or_else(|| "7d".into());
    let pool = &state.pool;

    // Resolve the first site.
    let site = first_site_id(pool).await?;

    let config = if let Some(site_id) = site {
        let today = Local::now().date_naive();
        let start = range_start(&range, today);

        let rows = daily_energy_totals(pool, site_id, start, today).await?;

        let labels: Vec<String> = rows.iter().map(|r| r.day.format("%b %d").to_string()).collect();
        let actual: Vec<f64> = rows.iter().map(|r| r.total_kwh).collect();
        // No baseline model yet -- pass empty predicted series.
        let predicted: Vec<f64> = Vec::new();

        charts::energy_usage_chart(labels, actual, predicted)
    } else {
        charts::energy_usage_chart(vec![], vec![], vec![])
    };

    Ok(ChartPartial {
        id: "energy-usage-chart".into(),
        config_json: charts::to_chart_json(&config),
        height: "300px".into(),
    })
}

async fn circuits_partial(
    State(_state): State<AppState>,
    Query(_query): Query<ChartRangeQuery>,
) -> Result<ChartPartial, WebError> {
    let config = charts::circuit_breakdown_chart(vec![], vec![]);

    Ok(ChartPartial {
        id: "circuit-breakdown-chart".into(),
        config_json: charts::to_chart_json(&config),
        height: "250px".into(),
    })
}

async fn bills_chart_partial(
    State(state): State<AppState>,
) -> Result<ChartPartial, WebError> {
    let pool = &state.pool;

    let site = first_site_id(pool).await?;

    let config = if let Some(site_id) = site {
        let accounts = lothal_db::bill::list_utility_accounts_by_site(pool, site_id).await?;

        // Accumulate bill totals by (year-month, utility_type).
        let mut monthly: BTreeMap<String, (f64, f64, f64)> = BTreeMap::new();

        for account in &accounts {
            let bills = lothal_db::bill::list_bills_by_account(pool, account.id).await?;
            for bill in &bills {
                let month_key = format!(
                    "{}-{:02}",
                    bill.period.range.start.year(),
                    bill.period.range.start.month(),
                );
                let entry = monthly.entry(month_key).or_insert((0.0, 0.0, 0.0));
                let amount = bill.total_amount.value();
                match account.utility_type {
                    UtilityType::Electric => entry.0 += amount,
                    UtilityType::Gas | UtilityType::Propane => entry.1 += amount,
                    UtilityType::Water | UtilityType::Sewer => entry.2 += amount,
                    _ => {} // skip internet, trash, etc.
                }
            }
        }

        let months: Vec<String> = monthly.keys().cloned().collect();
        let electric: Vec<f64> = monthly.values().map(|v| v.0).collect();
        let gas: Vec<f64> = monthly.values().map(|v| v.1).collect();
        let water: Vec<f64> = monthly.values().map(|v| v.2).collect();

        charts::bills_stacked_chart(months, electric, gas, water)
    } else {
        charts::bills_stacked_chart(vec![], vec![], vec![], vec![])
    };

    Ok(ChartPartial {
        id: "bills-chart".into(),
        config_json: charts::to_chart_json(&config),
        height: "300px".into(),
    })
}

#[derive(Deserialize)]
pub struct SimulateForm {
    pub scenario: Option<String>,
}

async fn simulate_partial(
    State(_state): State<AppState>,
    axum::Form(_form): axum::Form<SimulateForm>,
) -> Result<Html<String>, WebError> {
    Ok(Html(
        r#"<div class="bg-[#1a1d27] rounded-xl p-6 border border-[#2e3346]">
            <p class="text-[#8b8fa3] text-sm">Simulation results will appear here. Connect your site data first.</p>
        </div>"#
            .into(),
    ))
}

#[derive(Deserialize)]
pub struct ChatMessage {
    pub message: Option<String>,
}

async fn chat_send_partial(
    State(_state): State<AppState>,
    axum::Form(form): axum::Form<ChatMessage>,
) -> Result<Html<String>, WebError> {
    let user_msg = form.message.unwrap_or_default();
    if user_msg.is_empty() {
        return Ok(Html(String::new()));
    }

    let user_bubble = format!(
        r#"<div class="flex justify-end mb-4">
            <div class="bg-blue-600/20 border border-blue-500/30 rounded-xl px-4 py-3 max-w-[80%]">
                <p class="text-sm text-[#e8eaed]">{}</p>
            </div>
        </div>"#,
        html_escape(&user_msg),
    );

    let client = match LlmClient::from_env() {
        Ok(c) => c,
        Err(_) => {
            return Ok(Html(format!(
                r#"{user_bubble}
        <div class="flex justify-start mb-4">
            <div class="bg-[#232736] rounded-xl px-4 py-3 max-w-[80%] border border-[#2e3346]">
                <p class="text-sm text-[#8b8fa3]">LLM not configured. Set LOTHAL_LLM_PROVIDER and the appropriate API key.</p>
            </div>
        </div>"#,
            )));
        }
    };

    let request = CompletionRequest {
        system: "You are Lothal, a property intelligence agent. Answer questions about the \
                 user's property including energy usage, bills, water systems, livestock, \
                 garden, and recommendations. Be specific with numbers when available."
            .into(),
        messages: vec![Message {
            role: Role::User,
            content: user_msg,
        }],
        max_tokens: 500,
        temperature: 0.3,
    };

    let assistant_text = match client.complete(&request).await {
        Ok(resp) => resp.content,
        Err(e) => {
            tracing::error!(error = %e, "LLM completion failed");
            format!("Sorry, I couldn't process that request: {e}")
        }
    };

    Ok(Html(format!(
        r#"{user_bubble}
        <div class="flex justify-start mb-4">
            <div class="bg-[#232736] rounded-xl px-4 py-3 max-w-[80%] border border-[#2e3346]">
                <p class="text-sm text-[#8b8fa3]">{}</p>
            </div>
        </div>"#,
        html_escape(&assistant_text),
    )))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Return the first site ID or `None` when no sites exist.
async fn first_site_id(pool: &PgPool) -> Result<Option<Uuid>, sqlx::Error> {
    let sites = lothal_db::site::list_sites(pool).await?;
    Ok(sites.first().map(|s| s.id))
}

/// Map a range string like "24h", "7d", "30d", "1y" to a start date.
fn range_start(range: &str, today: NaiveDate) -> NaiveDate {
    match range {
        "24h" => today - chrono::Duration::days(1),
        "7d" => today - chrono::Duration::days(7),
        "30d" => today - chrono::Duration::days(30),
        "1y" => today - chrono::Duration::days(365),
        _ => today - chrono::Duration::days(7),
    }
}

/// Row returned by the daily energy aggregation query.
struct DailyEnergy {
    day: NaiveDate,
    total_kwh: f64,
}

/// Query the `readings_daily` continuous aggregate for site-level daily kWh.
async fn daily_energy_totals(
    pool: &PgPool,
    site_id: Uuid,
    start: NaiveDate,
    end: NaiveDate,
) -> Result<Vec<DailyEnergy>, sqlx::Error> {
    let rows: Vec<(NaiveDate, f64)> = sqlx::query_as(
        r#"SELECT rd.bucket::date as day, SUM(rd.sum_value) as total_kwh
           FROM readings_daily rd
           JOIN circuits c ON rd.source_id = c.id AND rd.source_type = 'circuit'
           JOIN panels p ON c.panel_id = p.id
           JOIN structures s ON p.structure_id = s.id
           WHERE s.site_id = $1
             AND rd.kind = 'electric_kwh'
             AND rd.bucket >= $2
             AND rd.bucket < $3
           GROUP BY rd.bucket::date
           ORDER BY day"#,
    )
    .bind(site_id)
    .bind(start)
    .bind(end)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|(day, total_kwh)| DailyEnergy { day, total_kwh })
        .collect())
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
