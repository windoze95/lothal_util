use anyhow::{Context, Result};
use chrono::NaiveDate;
use comfy_table::{modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL, Cell, ContentArrangement, Table};
use dialoguer::{Input, Select};
use sqlx::PgPool;
use uuid::Uuid;

use lothal_core::ontology::experiment::{
    Experiment, ExperimentStatus, Hypothesis, HypothesisCategory, Intervention,
};
use lothal_core::temporal::DateRange;
use lothal_core::units::Usd;
use lothal_engine::baseline::{self, BaselineMode, DailyDataPoint};

// ---------------------------------------------------------------------------
// Create experiment (interactive)
// ---------------------------------------------------------------------------

/// Interactive flow to create a new experiment.
///
///   1. Create a hypothesis: title, description, category, expected savings.
///   2. Create an intervention: description, date applied, cost, device.
///   3. Define baseline period (start/end).
///   4. Define result period (start/end).
///   5. Persist everything to the database.
pub async fn create_experiment(pool: &PgPool) -> Result<()> {
    println!("=== Create New Experiment ===");
    println!();

    // Resolve site.
    let sites = lothal_db::site::list_sites(pool).await?;
    let site = sites
        .first()
        .context("No sites found. Run `lothal init` first.")?;
    let site_id = site.id;

    // ----- Hypothesis -----
    println!("--- Step 1: Hypothesis ---");

    let title: String = Input::new()
        .with_prompt("Hypothesis title")
        .interact_text()?;

    let description: String = Input::new()
        .with_prompt("Description")
        .interact_text()?;

    let category_labels = [
        "Device Swap",
        "Behavior Change",
        "Envelope Upgrade",
        "Rate Optimization",
        "Load Shifting",
        "Maintenance",
        "Other",
    ];
    let cat_idx = Select::new()
        .with_prompt("Category")
        .items(&category_labels)
        .default(0)
        .interact()?;

    let category = match cat_idx {
        0 => HypothesisCategory::DeviceSwap,
        1 => HypothesisCategory::BehaviorChange,
        2 => HypothesisCategory::EnvelopeUpgrade,
        3 => HypothesisCategory::RateOptimization,
        4 => HypothesisCategory::LoadShifting,
        5 => HypothesisCategory::Maintenance,
        _ => HypothesisCategory::Other,
    };

    let expected_pct: String = Input::new()
        .with_prompt("Expected savings % (blank to skip)")
        .allow_empty(true)
        .interact_text()?;
    let expected_savings_pct: Option<f64> = if expected_pct.is_empty() {
        None
    } else {
        Some(expected_pct.parse().context("Expected a number for savings %")?)
    };

    let expected_usd_str: String = Input::new()
        .with_prompt("Expected annual savings $ (blank to skip)")
        .allow_empty(true)
        .interact_text()?;
    let expected_savings_usd: Option<Usd> = if expected_usd_str.is_empty() {
        None
    } else {
        Some(Usd::new(
            expected_usd_str
                .parse()
                .context("Expected a number for savings $")?,
        ))
    };

    let mut hypothesis = Hypothesis::new(site_id, title, description, category);
    hypothesis.expected_savings_pct = expected_savings_pct;
    hypothesis.expected_savings_usd = expected_savings_usd;

    println!();

    // ----- Intervention -----
    println!("--- Step 2: Intervention ---");

    let int_description: String = Input::new()
        .with_prompt("Intervention description")
        .interact_text()?;

    let date_str: String = Input::new()
        .with_prompt("Date applied (YYYY-MM-DD)")
        .interact_text()?;
    let date_applied = NaiveDate::parse_from_str(&date_str, "%Y-%m-%d")
        .context("Invalid date format, expected YYYY-MM-DD")?;

    let cost_str: String = Input::new()
        .with_prompt("Cost $ (blank if no cost)")
        .allow_empty(true)
        .interact_text()?;
    let cost: Option<Usd> = if cost_str.is_empty() {
        None
    } else {
        Some(Usd::new(cost_str.parse().context("Expected a number for cost")?))
    };

    let device_str: String = Input::new()
        .with_prompt("Device UUID (blank if not device-specific)")
        .allow_empty(true)
        .interact_text()?;
    let device_id: Option<Uuid> = if device_str.is_empty() {
        None
    } else {
        Some(device_str.parse().context("Invalid device UUID")?)
    };

    let mut intervention = Intervention::new(site_id, int_description, date_applied);
    intervention.cost = cost;
    intervention.device_id = device_id;

    println!();

    // ----- Baseline period -----
    println!("--- Step 3: Baseline Period ---");
    println!("(The period BEFORE the intervention for comparison)");

    let bl_start_str: String = Input::new()
        .with_prompt("Baseline start (YYYY-MM-DD)")
        .interact_text()?;
    let bl_start = NaiveDate::parse_from_str(&bl_start_str, "%Y-%m-%d")
        .context("Invalid date format")?;

    let bl_end_str: String = Input::new()
        .with_prompt("Baseline end (YYYY-MM-DD)")
        .interact_text()?;
    let bl_end = NaiveDate::parse_from_str(&bl_end_str, "%Y-%m-%d")
        .context("Invalid date format")?;

    let baseline_period = DateRange::new(bl_start, bl_end);

    println!();

    // ----- Result period -----
    println!("--- Step 4: Result Period ---");
    println!("(The period AFTER the intervention to measure impact)");

    let res_start_str: String = Input::new()
        .with_prompt("Result start (YYYY-MM-DD)")
        .interact_text()?;
    let res_start = NaiveDate::parse_from_str(&res_start_str, "%Y-%m-%d")
        .context("Invalid date format")?;

    let res_end_str: String = Input::new()
        .with_prompt("Result end (YYYY-MM-DD)")
        .interact_text()?;
    let res_end = NaiveDate::parse_from_str(&res_end_str, "%Y-%m-%d")
        .context("Invalid date format")?;

    let result_period = DateRange::new(res_start, res_end);

    // ----- Save -----
    println!();
    println!("Saving experiment...");

    lothal_db::experiment::insert_hypothesis(pool, &hypothesis).await?;
    lothal_db::experiment::insert_intervention(pool, &intervention).await?;

    let experiment = Experiment::new(
        site_id,
        hypothesis.id,
        intervention.id,
        baseline_period,
        result_period,
    );
    lothal_db::experiment::insert_experiment(pool, &experiment).await?;

    println!();
    println!("Experiment created successfully!");
    println!("  ID:          {}", experiment.id);
    println!("  Hypothesis:  {}", hypothesis.title);
    println!("  Status:      {}", experiment.status);
    println!("  Baseline:    {}", experiment.baseline_period);
    println!("  Result:      {}", experiment.result_period);

    Ok(())
}

// ---------------------------------------------------------------------------
// List experiments
// ---------------------------------------------------------------------------

/// Display a table of all experiments for the current site.
pub async fn list_experiments(pool: &PgPool) -> Result<()> {
    let sites = lothal_db::site::list_sites(pool).await?;
    let site = sites
        .first()
        .context("No sites found.")?;

    let experiments =
        lothal_db::experiment::list_experiments_by_site(pool, site.id).await?;
    let hypotheses =
        lothal_db::experiment::list_hypotheses_by_site(pool, site.id).await?;

    if experiments.is_empty() {
        println!("No experiments found. Create one with `lothal experiment create`.");
        return Ok(());
    }

    // Build a lookup from hypothesis_id -> title.
    let hyp_map: std::collections::HashMap<Uuid, &str> = hypotheses
        .iter()
        .map(|h| (h.id, h.title.as_str()))
        .collect();

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec![
            Cell::new("ID"),
            Cell::new("Hypothesis"),
            Cell::new("Status"),
            Cell::new("Baseline Period"),
            Cell::new("Result Period"),
            Cell::new("Savings"),
        ]);

    for exp in &experiments {
        let hyp_title = hyp_map
            .get(&exp.hypothesis_id)
            .copied()
            .unwrap_or("(unknown)");

        let savings = match (exp.actual_savings_pct, exp.actual_savings_usd) {
            (Some(pct), Some(usd)) => format!("{:.1}% (${:.2})", pct, usd.value()),
            (Some(pct), None) => format!("{:.1}%", pct),
            (None, Some(usd)) => format!("${:.2}", usd.value()),
            (None, None) => "-".into(),
        };

        table.add_row(vec![
            Cell::new(exp.id.to_string().split('-').next().unwrap_or("")),
            Cell::new(truncate(hyp_title, 30)),
            Cell::new(exp.status.to_string()),
            Cell::new(format!("{}", exp.baseline_period)),
            Cell::new(format!("{}", exp.result_period)),
            Cell::new(savings),
        ]);
    }

    println!("=== Experiments ===");
    println!("{table}");
    println!("{} experiment(s)", experiments.len());

    Ok(())
}

// ---------------------------------------------------------------------------
// Show experiment detail
// ---------------------------------------------------------------------------

/// Display detailed information about a single experiment.
pub async fn show_experiment(pool: &PgPool, id: &str) -> Result<()> {
    let exp_id: Uuid = resolve_experiment_id(pool, id).await?;

    let experiment = lothal_db::experiment::get_experiment(pool, exp_id)
        .await?
        .context("Experiment not found")?;

    // Look up the associated hypothesis.
    let sites = lothal_db::site::list_sites(pool).await?;
    let site = sites.first().context("No sites found")?;
    let hypotheses =
        lothal_db::experiment::list_hypotheses_by_site(pool, site.id).await?;
    let hypothesis = hypotheses
        .iter()
        .find(|h| h.id == experiment.hypothesis_id);

    println!("=== Experiment Detail ===");
    println!();

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec![Cell::new("Field"), Cell::new("Value")]);

    table.add_row(vec![
        Cell::new("ID"),
        Cell::new(experiment.id.to_string()),
    ]);
    table.add_row(vec![
        Cell::new("Status"),
        Cell::new(experiment.status.to_string()),
    ]);
    table.add_row(vec![
        Cell::new("Created"),
        Cell::new(experiment.created_at.format("%Y-%m-%d %H:%M").to_string()),
    ]);
    table.add_row(vec![
        Cell::new("Updated"),
        Cell::new(experiment.updated_at.format("%Y-%m-%d %H:%M").to_string()),
    ]);

    if let Some(hyp) = hypothesis {
        table.add_row(vec![Cell::new(""), Cell::new("")]);
        table.add_row(vec![
            Cell::new("Hypothesis"),
            Cell::new(&hyp.title),
        ]);
        table.add_row(vec![
            Cell::new("Description"),
            Cell::new(&hyp.description),
        ]);
        table.add_row(vec![
            Cell::new("Category"),
            Cell::new(hyp.category.to_string()),
        ]);
        if let Some(pct) = hyp.expected_savings_pct {
            table.add_row(vec![
                Cell::new("Expected savings %"),
                Cell::new(format!("{pct:.1}%")),
            ]);
        }
        if let Some(usd) = hyp.expected_savings_usd {
            table.add_row(vec![
                Cell::new("Expected savings $"),
                Cell::new(format!("${:.2}", usd.value())),
            ]);
        }
    }

    table.add_row(vec![Cell::new(""), Cell::new("")]);
    table.add_row(vec![
        Cell::new("Baseline period"),
        Cell::new(format!(
            "{} ({} days)",
            experiment.baseline_period,
            experiment.baseline_period.days(),
        )),
    ]);
    table.add_row(vec![
        Cell::new("Result period"),
        Cell::new(format!(
            "{} ({} days)",
            experiment.result_period,
            experiment.result_period.days(),
        )),
    ]);

    if let Some(pct) = experiment.actual_savings_pct {
        table.add_row(vec![Cell::new(""), Cell::new("")]);
        table.add_row(vec![
            Cell::new("Actual savings %"),
            Cell::new(format!("{pct:.1}%")),
        ]);
    }
    if let Some(usd) = experiment.actual_savings_usd {
        table.add_row(vec![
            Cell::new("Actual savings $"),
            Cell::new(format!("${:.2}", usd.value())),
        ]);
    }
    if let Some(conf) = experiment.confidence {
        table.add_row(vec![
            Cell::new("Confidence"),
            Cell::new(format!("{:.0}%", conf * 100.0)),
        ]);
    }
    if let Some(ref notes) = experiment.notes {
        table.add_row(vec![
            Cell::new("Notes"),
            Cell::new(notes),
        ]);
    }

    println!("{table}");

    Ok(())
}

// ---------------------------------------------------------------------------
// Evaluate experiment
// ---------------------------------------------------------------------------

/// Evaluate an experiment by computing weather-normalized usage change between
/// the baseline and result periods.
///
///   1. Fetch the experiment record.
///   2. Load bill and weather data for both periods.
///   3. Build daily data points for baseline and result.
///   4. Compute a baseline model from the baseline-period data.
///   5. Use the engine to produce a weather-normalized evaluation.
///   6. Update the experiment record in the database.
pub async fn evaluate_experiment_cmd(pool: &PgPool, id: &str) -> Result<()> {
    let exp_id = resolve_experiment_id(pool, id).await?;

    let experiment = lothal_db::experiment::get_experiment(pool, exp_id)
        .await?
        .context("Experiment not found")?;

    let sites = lothal_db::site::list_sites(pool).await?;
    let site = sites.first().context("No sites found")?;

    println!("=== Evaluating Experiment {} ===", short_id(experiment.id));
    println!();

    // ----- Load bills -----
    let accounts =
        lothal_db::bill::list_utility_accounts_by_site(pool, site.id).await?;
    let electric_account = accounts
        .iter()
        .find(|a| a.utility_type == lothal_core::ontology::utility::UtilityType::Electric)
        .context("No electric utility account found")?;

    let all_bills =
        lothal_db::bill::list_bills_by_account(pool, electric_account.id).await?;

    // Compute effective rate from latest bill.
    let rate_per_kwh = all_bills
        .last()
        .and_then(|b| b.effective_rate())
        .map(|r| r.value())
        .unwrap_or(0.10);

    // ----- Build data points for baseline period -----
    let base_temp = 65.0;

    let baseline_weather = lothal_db::weather::get_daily_weather_summaries(
        pool,
        site.id,
        experiment.baseline_period.start,
        experiment.baseline_period.end,
    )
    .await?;

    let result_weather = lothal_db::weather::get_daily_weather_summaries(
        pool,
        site.id,
        experiment.result_period.start,
        experiment.result_period.end,
    )
    .await?;

    let baseline_data = build_daily_data(
        &all_bills,
        &baseline_weather,
        &experiment.baseline_period,
        base_temp,
    );
    let result_data = build_daily_data(
        &all_bills,
        &result_weather,
        &experiment.result_period,
        base_temp,
    );

    println!("  Baseline data points: {}", baseline_data.len());
    println!("  Result data points:   {}", result_data.len());

    if baseline_data.len() < 3 {
        println!("\nInsufficient baseline data to evaluate. Need at least 3 days.");
        return Ok(());
    }
    if result_data.is_empty() {
        println!("\nNo result data available for the result period.");
        return Ok(());
    }

    // ----- Compute baseline model -----
    // Use the dominant mode (cooling or heating) based on which has more
    // non-zero degree-day data in the baseline.
    let cooling_count = baseline_data
        .iter()
        .filter(|d| d.cooling_degree_days > 0.0)
        .count();
    let heating_count = baseline_data
        .iter()
        .filter(|d| d.heating_degree_days > 0.0)
        .count();
    let mode = if cooling_count >= heating_count {
        BaselineMode::Cooling
    } else {
        BaselineMode::Heating
    };

    let baseline_model = baseline::compute_baseline(&baseline_data, mode)
        .context("Failed to compute baseline model")?;

    println!("  Baseline model R-squared: {:.4}", baseline_model.r_squared);
    println!();

    // ----- Evaluate -----
    let eval = lothal_engine::experiment::evaluate_experiment(
        &baseline_data,
        &result_data,
        Some(&baseline_model),
        rate_per_kwh,
    )
    .context("Experiment evaluation failed")?;

    // ----- Display results -----
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec![Cell::new("Metric"), Cell::new("Value")]);

    table.add_row(vec![
        Cell::new("Baseline avg daily usage"),
        Cell::new(format!("{:.2} kWh", eval.baseline_avg_daily_usage)),
    ]);
    table.add_row(vec![
        Cell::new("Result avg daily usage"),
        Cell::new(format!("{:.2} kWh", eval.result_avg_daily_usage)),
    ]);
    table.add_row(vec![
        Cell::new("Raw change"),
        Cell::new(format!("{:+.1}%", eval.raw_change_pct)),
    ]);
    table.add_row(vec![
        Cell::new("Weather-normalized change"),
        Cell::new(match eval.weather_normalized_change_pct {
            Some(v) => format!("{:+.1}%", v),
            None => "N/A".to_string(),
        }),
    ]);
    table.add_row(vec![
        Cell::new("Est. annual savings"),
        Cell::new(format!("${:.2}", eval.estimated_annual_savings_usd.value())),
    ]);
    table.add_row(vec![
        Cell::new("Confidence score"),
        Cell::new(format!("{:.0}%", eval.confidence_score * 100.0)),
    ]);
    table.add_row(vec![
        Cell::new("Interpretation"),
        Cell::new(&eval.interpretation),
    ]);

    println!("{table}");

    // ----- Update experiment in DB -----
    let mut updated = experiment.clone();
    updated.status = if eval.confidence_score >= 0.5 {
        ExperimentStatus::Completed
    } else {
        ExperimentStatus::Inconclusive
    };
    updated.actual_savings_pct = eval.weather_normalized_change_pct.or(Some(eval.raw_change_pct));
    updated.actual_savings_usd = Some(eval.estimated_annual_savings_usd);
    updated.confidence = Some(eval.confidence_score);
    updated.notes = Some(eval.interpretation.clone());
    updated.updated_at = chrono::Utc::now();

    lothal_db::experiment::update_experiment(pool, &updated).await?;

    println!();
    println!(
        "Experiment {} updated to status: {}",
        short_id(updated.id),
        updated.status,
    );

    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build `DailyDataPoint` records for a given period by spreading bill usage
/// across days and pairing with weather summaries.
fn build_daily_data(
    bills: &[lothal_core::ontology::bill::Bill],
    weather: &[lothal_db::weather::DailyWeatherRow],
    period: &DateRange,
    base_temp: f64,
) -> Vec<DailyDataPoint> {
    let weather_map: std::collections::HashMap<chrono::NaiveDate, &lothal_db::weather::DailyWeatherRow> =
        weather.iter().map(|w| (w.date, w)).collect();

    let mut points = Vec::new();

    for bill in bills {
        let daily_usage = match bill.daily_usage() {
            Some(u) => u,
            None => continue,
        };

        for date in bill.period.range.iter_days() {
            if !period.contains_date(date) {
                continue;
            }
            if let Some(w) = weather_map.get(&date) {
                let cdd = (w.avg_temp_f - base_temp).max(0.0);
                let hdd = (base_temp - w.avg_temp_f).max(0.0);
                points.push(DailyDataPoint {
                    date,
                    usage: daily_usage,
                    cooling_degree_days: cdd,
                    heating_degree_days: hdd,
                });
            }
        }
    }

    points
}

/// Attempt to resolve a (possibly short) experiment ID to a full UUID.
///
/// If the user supplies a full UUID, it is parsed directly. Otherwise, we look
/// up experiments whose ID starts with the given prefix.
async fn resolve_experiment_id(pool: &PgPool, id: &str) -> Result<Uuid> {
    // Try parsing as a full UUID first.
    if let Ok(uuid) = id.parse::<Uuid>() {
        return Ok(uuid);
    }

    // Prefix search.
    let sites = lothal_db::site::list_sites(pool).await?;
    let site = sites.first().context("No sites found")?;
    let experiments =
        lothal_db::experiment::list_experiments_by_site(pool, site.id).await?;

    let matches: Vec<_> = experiments
        .iter()
        .filter(|e| e.id.to_string().starts_with(id))
        .collect();

    match matches.len() {
        0 => anyhow::bail!("No experiment found matching '{id}'"),
        1 => Ok(matches[0].id),
        n => anyhow::bail!(
            "Ambiguous experiment ID '{id}' matches {n} experiments. Use a longer prefix."
        ),
    }
}

/// Return the first 8 characters of a UUID for display.
fn short_id(id: Uuid) -> String {
    id.to_string()[..8].to_string()
}

/// Truncate a string to at most `max` characters, appending "..." if trimmed.
fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max.saturating_sub(3)])
    }
}
