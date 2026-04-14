//! Scheduler daemon: runs periodic tasks (weather pull, email ingest, anomaly
//! sweep, daily briefing) as a long-running foreground process. Intended to be
//! managed by systemd via `deploy/lothal.service`.

use std::time::Duration;

use anyhow::{Context, Result};
use chrono::{DateTime, Datelike, Local, NaiveDate, TimeZone, Timelike, Utc, Weekday};
use sqlx::PgPool;
use tracing::{error, info, warn};
use uuid::Uuid;

use lothal_ai::briefing::format::BriefingOutput;
use lothal_ai::provider::LlmClient;
use lothal_ingest::nws::NwsConfig;

// ---------------------------------------------------------------------------
// Task scheduling
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
enum Cadence {
    /// Run every `Duration` since last completion.
    Every(Duration),
    /// Run once per day at the given local-time hour and minute.
    DailyAt { hour: u32, minute: u32 },
    /// Run once per week at the given weekday and local-time.
    #[allow(dead_code)]
    WeeklyAt {
        weekday: Weekday,
        hour: u32,
        minute: u32,
    },
}

struct ScheduledTask {
    name: &'static str,
    cadence: Cadence,
}

const TASKS: &[ScheduledTask] = &[
    ScheduledTask {
        name: "weather_pull",
        cadence: Cadence::Every(Duration::from_secs(60 * 60)),
    },
    ScheduledTask {
        name: "email_ingest",
        cadence: Cadence::Every(Duration::from_secs(30 * 60)),
    },
    ScheduledTask {
        name: "anomaly_sweep",
        cadence: Cadence::Every(Duration::from_secs(15 * 60)),
    },
    ScheduledTask {
        name: "daily_briefing",
        cadence: Cadence::DailyAt {
            hour: 6,
            minute: 15,
        },
    },
];

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

pub async fn run(pool: PgPool) -> Result<()> {
    info!("lothal daemon starting");

    // Fail fast on missing config so systemd reports startup failure.
    let sites = lothal_db::site::list_sites(&pool).await?;
    let site = sites
        .first()
        .context("No sites configured. Run `lothal init` first before starting the daemon.")?
        .clone();
    info!(site = %site.address, "daemon attached to site");

    let mut ticker = tokio::time::interval(Duration::from_secs(60));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        ticker.tick().await;

        for task in TASKS {
            let due = match is_due(&pool, task).await {
                Ok(v) => v,
                Err(e) => {
                    warn!(task = task.name, error = %e, "failed to check task due-ness");
                    continue;
                }
            };
            if !due {
                continue;
            }

            let start = std::time::Instant::now();
            let result = dispatch(task.name, &pool, &site.id).await;
            let duration_ms = start.elapsed().as_millis() as i32;

            let (status, error_msg) = match &result {
                Ok(()) => ("ok", None),
                Err(e) => ("error", Some(e.to_string())),
            };

            if let Err(e) = record_run(&pool, task.name, status, error_msg.as_deref(), duration_ms)
                .await
            {
                error!(task = task.name, error = %e, "failed to record scheduler run");
            }

            match result {
                Ok(()) => info!(task = task.name, duration_ms, "task ok"),
                Err(e) => error!(task = task.name, duration_ms, error = %e, "task failed"),
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Due-ness check
// ---------------------------------------------------------------------------

async fn is_due(pool: &PgPool, task: &ScheduledTask) -> Result<bool> {
    let last_run = fetch_last_run(pool, task.name).await?;
    let now_local = Local::now();

    match task.cadence {
        Cadence::Every(interval) => match last_run {
            None => Ok(true),
            Some(last) => {
                let elapsed = Utc::now().signed_duration_since(last);
                let interval_secs = interval.as_secs() as i64;
                Ok(elapsed.num_seconds() >= interval_secs)
            }
        },
        Cadence::DailyAt { hour, minute } => {
            let target_today = Local
                .with_ymd_and_hms(
                    now_local.year(),
                    now_local.month(),
                    now_local.day(),
                    hour,
                    minute,
                    0,
                )
                .single()
                .context("invalid daily-at time")?;
            if now_local < target_today {
                return Ok(false);
            }
            match last_run {
                None => Ok(true),
                Some(last) => {
                    let last_local = last.with_timezone(&Local);
                    Ok(last_local.date_naive() < now_local.date_naive())
                }
            }
        }
        Cadence::WeeklyAt {
            weekday,
            hour,
            minute,
        } => {
            if now_local.weekday() != weekday {
                return Ok(false);
            }
            let target_today = Local
                .with_ymd_and_hms(
                    now_local.year(),
                    now_local.month(),
                    now_local.day(),
                    hour,
                    minute,
                    0,
                )
                .single()
                .context("invalid weekly-at time")?;
            if now_local < target_today {
                return Ok(false);
            }
            match last_run {
                None => Ok(true),
                Some(last) => {
                    let last_local = last.with_timezone(&Local);
                    Ok(last_local.date_naive() < now_local.date_naive())
                }
            }
        }
    }
}

async fn fetch_last_run(pool: &PgPool, task_name: &str) -> Result<Option<DateTime<Utc>>> {
    let row = sqlx::query_as::<_, (DateTime<Utc>,)>(
        "SELECT last_run FROM scheduler_runs WHERE task_name = $1",
    )
    .bind(task_name)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(t,)| t))
}

async fn record_run(
    pool: &PgPool,
    task_name: &str,
    status: &str,
    error: Option<&str>,
    duration_ms: i32,
) -> Result<()> {
    sqlx::query(
        r#"INSERT INTO scheduler_runs (task_name, last_run, last_status, last_error, duration_ms)
           VALUES ($1, now(), $2, $3, $4)
           ON CONFLICT (task_name) DO UPDATE SET
               last_run = EXCLUDED.last_run,
               last_status = EXCLUDED.last_status,
               last_error = EXCLUDED.last_error,
               duration_ms = EXCLUDED.duration_ms"#,
    )
    .bind(task_name)
    .bind(status)
    .bind(error)
    .bind(duration_ms)
    .execute(pool)
    .await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Task dispatch
// ---------------------------------------------------------------------------

async fn dispatch(name: &str, pool: &PgPool, site_id: &Uuid) -> Result<()> {
    match name {
        "weather_pull" => run_weather_pull(pool, site_id).await,
        "email_ingest" => run_email_ingest(pool).await,
        "anomaly_sweep" => run_anomaly_sweep(pool, site_id).await,
        "daily_briefing" => run_daily_briefing(pool, site_id).await,
        other => Err(anyhow::anyhow!("unknown task: {other}")),
    }
}

// ---------------------------------------------------------------------------
// Weather pull
// ---------------------------------------------------------------------------

async fn run_weather_pull(pool: &PgPool, site_id: &Uuid) -> Result<()> {
    let station = std::env::var("NWS_STATION").unwrap_or_else(|_| "KGOK".into());
    let config = NwsConfig {
        station_id: station,
        user_agent: "lothal-daemon/0.1 (github.com/lothal)".into(),
    };

    let end = Utc::now();
    let start = end - chrono::Duration::hours(2);
    let mut observations = lothal_ingest::nws::fetch_observations_range(&config, start, end).await?;

    for obs in &mut observations {
        obs.site_id = *site_id;
    }

    let count = observations.len();
    lothal_db::weather::insert_weather_batch(pool, &observations).await?;
    info!(count, "weather_pull inserted observations");
    Ok(())
}

// ---------------------------------------------------------------------------
// Email ingest (one-shot — the daemon handles the cadence)
// ---------------------------------------------------------------------------

async fn run_email_ingest(pool: &PgPool) -> Result<()> {
    let config = match lothal_ai::extract::email::ImapConfig::from_env() {
        Ok(c) => c,
        Err(e) => {
            info!("email_ingest skipped — IMAP not configured: {e}");
            return Ok(());
        }
    };

    let sites = lothal_db::site::list_sites(pool).await?;
    let site = sites.first().context("No sites configured")?;
    let accounts = lothal_db::bill::list_utility_accounts_by_site(pool, site.id).await?;
    let account = match accounts.first() {
        Some(a) => a,
        None => {
            info!("email_ingest skipped — no utility accounts configured");
            return Ok(());
        }
    };

    let client = LlmClient::from_env()?;
    let results =
        lothal_ai::extract::email::poll_and_ingest(&config, pool, account.id, &client).await?;

    for result in &results {
        match &result.status {
            lothal_ai::extract::email::EmailStatus::Parsed { bill_id } => {
                info!(sender = %result.sender, %bill_id, "email_ingest parsed bill");
                lothal_db::ai::insert_email_ingest_log(
                    pool,
                    &result.message_id,
                    &result.sender,
                    result.subject.as_deref(),
                    Some(*bill_id),
                    "parsed",
                    None,
                )
                .await?;
            }
            lothal_ai::extract::email::EmailStatus::Skipped(reason) => {
                info!(sender = %result.sender, reason, "email_ingest skipped");
            }
            lothal_ai::extract::email::EmailStatus::Failed(err) => {
                warn!(sender = %result.sender, err, "email_ingest failed");
                lothal_db::ai::insert_email_ingest_log(
                    pool,
                    &result.message_id,
                    &result.sender,
                    result.subject.as_deref(),
                    None,
                    "failed",
                    Some(err),
                )
                .await?;
            }
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Anomaly sweep
// ---------------------------------------------------------------------------

async fn run_anomaly_sweep(pool: &PgPool, site_id: &Uuid) -> Result<()> {
    let yesterday = Local::now().date_naive() - chrono::Duration::days(1);

    let candidates = lothal_ai::anomaly::sweep(pool, *site_id, yesterday).await?;
    if candidates.is_empty() {
        return Ok(());
    }

    let to_alert = lothal_ai::anomaly::filter_duplicates(pool, candidates).await?;
    if to_alert.is_empty() {
        return Ok(());
    }

    info!(count = to_alert.len(), "anomaly_sweep detected new anomalies");
    lothal_ai::anomaly::persist(pool, &to_alert).await?;

    if let Ok(output) = BriefingOutput::from_env() {
        let message = format_anomaly_alert(yesterday, &to_alert);
        if let Err(e) = output.send(&message).await {
            warn!(error = %e, "anomaly alert delivery failed");
        }
    }

    Ok(())
}

fn format_anomaly_alert(date: NaiveDate, anomalies: &[lothal_ai::anomaly::Anomaly]) -> String {
    let header = format!("Lothal anomaly alert — {date}");
    let body: Vec<String> = anomalies.iter().map(|a| format!("• {}", a.message)).collect();
    format!("{header}\n\n{}", body.join("\n"))
}

// ---------------------------------------------------------------------------
// Daily briefing
// ---------------------------------------------------------------------------

async fn run_daily_briefing(pool: &PgPool, site_id: &Uuid) -> Result<()> {
    let yesterday = Local::now().date_naive() - chrono::Duration::days(1);

    let client = LlmClient::from_env()?;
    let invoker: std::sync::Arc<dyn lothal_ontology::llm_function::LlmInvoker> =
        std::sync::Arc::new(lothal_ai::LlmClientInvoker::new(client));
    let functions = lothal_ai::functions::default_registry(invoker);
    let content =
        lothal_ai::briefing::generate_briefing(pool, *site_id, yesterday, &functions).await?;

    if let Ok(output) = BriefingOutput::from_env() {
        if let Err(e) = output.send(&content).await {
            warn!(error = %e, "briefing delivery failed");
        }
    }

    info!(date = %yesterday, "daily briefing generated");
    Ok(())
}

// ---------------------------------------------------------------------------
// Allow suppressing the `Timelike` unused import if WeeklyAt is ever removed.
// ---------------------------------------------------------------------------

#[allow(dead_code)]
fn _suppress_unused_timelike(t: DateTime<Local>) -> u32 {
    t.hour()
}
