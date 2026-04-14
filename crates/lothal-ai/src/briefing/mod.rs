pub mod context;
pub mod format;

use sqlx::PgPool;
use uuid::Uuid;

use context::BriefingContext;
use lothal_ontology::llm_function::LlmFunctionRegistry;
use crate::AiError;

/// When yesterday's usage deviates from the weather-normalized baseline by
/// more than this fraction, switch to the `diagnose_briefing` function
/// (extended thinking enabled) instead of the `calm_briefing` function.
const DIAGNOSE_DEVIATION_THRESHOLD: f64 = 15.0;

/// Generate a daily briefing for a site via the `LlmFunctionRegistry`.
///
/// Picks `calm_briefing` vs `diagnose_briefing` based on deviation from the
/// weather-normalized baseline. Both prompts + budgets are declared by their
/// respective [`LlmFunction`][lothal_ontology::LlmFunction] impls; this call
/// site only picks the name and packages the prompt.
pub async fn generate_briefing(
    pool: &PgPool,
    site_id: Uuid,
    date: chrono::NaiveDate,
    functions: &LlmFunctionRegistry,
) -> Result<String, AiError> {
    let ctx = context::gather_context(pool, site_id, date).await?;
    let prompt = build_briefing_prompt(&ctx);

    let diagnose = ctx
        .baseline_comparison
        .as_ref()
        .map(|b| b.deviation_pct.abs() > DIAGNOSE_DEVIATION_THRESHOLD)
        .unwrap_or(false);

    let function_name = if diagnose { "diagnose_briefing" } else { "calm_briefing" };

    let call = functions
        .invoke(
            function_name,
            "briefing",
            pool.clone(),
            serde_json::json!({ "prompt": prompt }),
            None,
            None,
        )
        .await
        .map_err(|e| AiError::LlmRequest(format!("{function_name}: {e}")))?;

    let content = call
        .output
        .as_ref()
        .and_then(|v| v.0.get("briefing"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| AiError::LlmResponse("briefing function returned no `briefing`".into()))?
        .to_string();
    let model = call.model.clone().unwrap_or_default();

    store_briefing(pool, site_id, date, &content, &ctx, &model).await?;

    Ok(content)
}

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
