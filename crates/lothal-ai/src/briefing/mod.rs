pub mod context;
pub mod format;

use sqlx::PgPool;
use uuid::Uuid;

use context::BriefingContext;
use crate::provider::{CompletionRequest, LlmClient, Message, Role};
use crate::AiError;

/// Generate a daily briefing for a site.
pub async fn generate_briefing(
    pool: &PgPool,
    site_id: Uuid,
    date: chrono::NaiveDate,
    provider: &LlmClient,
) -> Result<String, AiError> {
    let ctx = context::gather_context(pool, site_id, date).await?;
    let prompt = build_briefing_prompt(&ctx);

    let request = CompletionRequest {
        system: BRIEFING_SYSTEM_PROMPT.to_string(),
        messages: vec![Message {
            role: Role::User,
            content: prompt,
        }],
        max_tokens: 512,
        temperature: 0.3,
        budget_tokens: None,
    };

    let response = provider.complete(&request).await?;

    // Store briefing in DB.
    store_briefing(pool, site_id, date, &response.content, &ctx, &response.model).await?;

    Ok(response.content)
}

const BRIEFING_SYSTEM_PROMPT: &str = "\
You are a property operations analyst. Given structured data about yesterday's \
energy, water, pool, livestock, garden, and weather, produce a concise daily \
briefing in 5-8 sentences covering multi-system status.

Rules:
- Lead with the headline number: total kWh and cost.
- Compare to the weather-normalized baseline if available.
- Explain any anomalies (high circuit runtime, unusual patterns).
- Include property operations: pool status, egg collection, septic alerts.
- Mention upcoming maintenance if due within 7 days.
- Be specific with numbers. No vague language.
- End with one actionable cross-system suggestion if applicable.
- If livestock or garden data is present, include a one-liner on each.";

fn build_briefing_prompt(ctx: &BriefingContext) -> String {
    let mut sections = Vec::new();

    sections.push(format!("Date: {}", ctx.date));

    if let Some(ref weather) = ctx.weather {
        sections.push(format!(
            "Weather: high {:.0}F, low {:.0}F, avg {:.1}F, CDD {:.1}, HDD {:.1}",
            weather.max_temp_f, weather.min_temp_f, weather.avg_temp_f,
            weather.cooling_degree_days, weather.heating_degree_days
        ));
    }

    if let Some(usage) = ctx.total_kwh {
        let cost_str = ctx
            .estimated_cost
            .map(|c| format!(", est. cost ${c:.2}"))
            .unwrap_or_default();
        sections.push(format!("Electric usage: {usage:.1} kWh{cost_str}"));
    }

    if let Some(ref baseline) = ctx.baseline_comparison {
        sections.push(format!(
            "Baseline comparison: {:.1} kWh predicted, {:.1}% {}",
            baseline.predicted_kwh,
            baseline.deviation_pct.abs(),
            if baseline.deviation_pct > 0.0 {
                "above"
            } else {
                "below"
            }
        ));
    }

    if !ctx.circuit_anomalies.is_empty() {
        let anomalies: Vec<String> = ctx
            .circuit_anomalies
            .iter()
            .map(|a| {
                format!(
                    "Circuit '{}': {:.1}h runtime vs {:.1}h avg",
                    a.circuit_label, a.actual_hours, a.avg_hours
                )
            })
            .collect();
        sections.push(format!("Anomalies: {}", anomalies.join("; ")));
    }

    if !ctx.maintenance_due.is_empty() {
        let items: Vec<String> = ctx
            .maintenance_due
            .iter()
            .map(|m| format!("{} (due {})", m.description, m.due_date))
            .collect();
        sections.push(format!("Maintenance due: {}", items.join("; ")));
    }

    if !ctx.active_experiments.is_empty() {
        let exps: Vec<String> = ctx
            .active_experiments
            .iter()
            .map(|e| e.title.clone())
            .collect();
        sections.push(format!("Active experiments: {}", exps.join(", ")));
    }

    // Property operations context
    if let Some(ref pool_status) = ctx.pool_status {
        let runtime = pool_status
            .pump_runtime_hours
            .map(|h| format!(", pump ran {h:.1}h"))
            .unwrap_or_default();
        sections.push(format!("Pool '{}'{runtime}", pool_status.pool_name));
    }

    if let Some(ref livestock) = ctx.livestock_summary {
        let mortality_note = if livestock.mortality > 0 {
            format!(", {} mortality", livestock.mortality)
        } else {
            String::new()
        };
        sections.push(format!(
            "Livestock '{}': {:.0} eggs, {:.1} lbs feed{mortality_note}",
            livestock.flock_name, livestock.eggs, livestock.feed_lbs
        ));
    }

    if let Some(ref septic) = ctx.septic_alert {
        let status = if septic.is_overdue {
            format!("OVERDUE by {} days", septic.days_until_pump.abs())
        } else {
            format!("due in {} days", septic.days_until_pump)
        };
        sections.push(format!("Septic pump-out: {status}"));
    }

    sections.join("\n")
}

async fn store_briefing(
    pool: &PgPool,
    site_id: Uuid,
    date: chrono::NaiveDate,
    content: &str,
    ctx: &BriefingContext,
    model: &str,
) -> Result<(), AiError> {
    let context_json = serde_json::to_value(ctx).unwrap_or(serde_json::Value::Null);

    sqlx::query(
        r#"INSERT INTO briefings (id, site_id, date, content, context, model, created_at)
           VALUES ($1, $2, $3, $4, $5, $6, now())
           ON CONFLICT (site_id, date) DO UPDATE
           SET content = $4, context = $5, model = $6, created_at = now()"#,
    )
    .bind(Uuid::new_v4())
    .bind(site_id)
    .bind(date)
    .bind(content)
    .bind(&context_json)
    .bind(model)
    .execute(pool)
    .await?;

    Ok(())
}
