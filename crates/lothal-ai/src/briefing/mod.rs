pub mod context;
pub mod format;

use sqlx::PgPool;
use uuid::Uuid;

use context::BriefingContext;
use crate::provider::{CompletionRequest, LlmClient, Message, Role};
use crate::AiError;

/// When yesterday's usage deviates from the weather-normalized baseline by
/// more than this fraction, switch to the "diagnose" briefing mode that
/// enables extended thinking and asks the model to reason about cause.
const DIAGNOSE_DEVIATION_THRESHOLD: f64 = 15.0;

/// Extended-thinking budget used in diagnose mode.
const DIAGNOSE_THINKING_BUDGET: u32 = 4000;

/// Generate a daily briefing for a site.
///
/// Uses a standard briefing prompt on calm days and switches to a "diagnose"
/// prompt with extended thinking when yesterday's usage deviates from the
/// weather-normalized baseline by more than 15%.
pub async fn generate_briefing(
    pool: &PgPool,
    site_id: Uuid,
    date: chrono::NaiveDate,
    provider: &LlmClient,
) -> Result<String, AiError> {
    let ctx = context::gather_context(pool, site_id, date).await?;
    let prompt = build_briefing_prompt(&ctx);

    let diagnose = ctx
        .baseline_comparison
        .as_ref()
        .map(|b| b.deviation_pct.abs() > DIAGNOSE_DEVIATION_THRESHOLD)
        .unwrap_or(false);

    let (system, max_tokens, budget_tokens) = if diagnose {
        (DIAGNOSE_SYSTEM_PROMPT, 1024, Some(DIAGNOSE_THINKING_BUDGET))
    } else {
        (BRIEFING_SYSTEM_PROMPT, 512, None)
    };

    let request = CompletionRequest {
        system: system.to_string(),
        messages: vec![Message {
            role: Role::User,
            content: prompt,
        }],
        max_tokens,
        temperature: 0.3,
        budget_tokens,
    };

    let response = provider.complete(&request).await?;

    store_briefing(pool, site_id, date, &response.content, &ctx, &response.model).await?;

    Ok(response.content)
}

const BRIEFING_SYSTEM_PROMPT: &str = "\
You are a property operations analyst producing a 5–8 sentence daily briefing \
for a single property owner. The property is in Guthrie, Oklahoma — humid \
subtropical climate, hot summers, mild winters, occasional severe weather.

Rules:
- Lead with the headline number: total kWh and estimated cost.
- Compare to the weather-normalized baseline when available.
- Call out any circuit anomalies with specific numbers.
- Mention property operations that matter today: pool status, egg count, septic alerts, garden activity.
- Flag maintenance due within 7 days.
- If an active experiment is relevant to yesterday's data, mention it.
- Be specific with numbers. Avoid vague words like \"notably\", \"slightly\", \"a bit\".
- End with ONE actionable cross-system suggestion, or omit the end-line if nothing actionable surfaces.
- Do not invent data. If a field is missing, omit it silently.

Example briefings for calm days:

Example 1:
\"Yesterday used 28.4 kWh ($3.12), 4% below the weather-normalized baseline of 29.6 kWh. Pool pump ran 6.2h (normal). Flock produced 4 eggs, consumed 1.8 lb feed. Septic pump-out due in 47 days. No anomalies. Worth replacing the coop heat lamp bulb next time you're in the feed store — current one is at 850 of its rated 1000 hours.\"

Example 2:
\"Yesterday used 41.7 kWh ($4.59), 8% above the baseline of 38.5 kWh on a 92°F day (CDD 27). Kitchen branch ran 18% above its 14-day average — likely the oven from the Sunday meal prep. Pool held 82°F, pump 8.1h. 3 eggs collected. No maintenance due this week.\"

Example 3:
\"Yesterday used 12.3 kWh ($1.35) on a mild 68°F day, right at baseline. Pool pump ran 5.5h. Flock: 4 eggs, 1.5 lb feed, no incidents. Active experiment 'setback thermostat schedule' is in day 4/14 — early signs show 6% savings on HVAC circuit.\"";

const DIAGNOSE_SYSTEM_PROMPT: &str = "\
You are a property operations analyst investigating a meaningful deviation \
from yesterday's weather-normalized energy baseline. The property is in \
Guthrie, Oklahoma.

Your job: produce a briefing that (a) states the deviation plainly, (b) \
reasons through the 2–3 most likely causes using the circuit-level data, \
weather, and active experiments, and (c) proposes the cheapest next step to \
confirm or rule out the most likely cause.

Rules:
- Lead with the headline: actual vs predicted kWh and the percentage deviation.
- Then diagnose. Be concrete — reference specific circuits and numbers.
- If an active experiment could explain the deviation, mention it before blaming a device.
- Distinguish 'a known cause' (experiment, heat wave, holiday) from 'an unexplained anomaly'.
- For unexplained anomalies, propose the cheapest diagnostic test — not a fix.
- 6–10 sentences. Be specific with numbers. Don't hedge with vague language.

Example diagnose briefing:

\"Yesterday used 52.1 kWh ($5.73), 22% above the weather-normalized baseline of 42.7 kWh on a 96°F day (CDD 31). The pool pump circuit ran 11.3h vs its 6.8h 14-day average — accounts for roughly 4.5 kWh of the 9.4 kWh overage. The HVAC circuit was only 4% high, consistent with the temperature. Active experiment 'pool cover on when not swimming' is suspended this week per your note — likely related. Cheapest check: verify the pool pump schedule in Home Assistant didn't revert; the solar cover being off also roughly doubles evaporative heat loss so the pump may be compensating. If the schedule is correct, the next signal would be a temperature sensor on the return line to see whether the pump is actually cooling.\"";

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
