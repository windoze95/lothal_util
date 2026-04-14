use std::path::Path;

use anyhow::{Context, Result};
use sqlx::PgPool;
use uuid::Uuid;

use lothal_ai::provider::LlmClient;

// ---------------------------------------------------------------------------
// Status
// ---------------------------------------------------------------------------

pub async fn check_status() -> Result<()> {
    let client = LlmClient::from_env()?;
    println!("Provider: {}", client.provider_name());
    println!("Model:    {}", client.model_name());
    println!("Checking connectivity...");

    match client.check_status().await {
        Ok(msg) => println!("  {msg}"),
        Err(e) => println!("  FAILED: {e}"),
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Parse Bill
// ---------------------------------------------------------------------------

pub async fn parse_bill(pool: &PgPool, path: &str, provider_override: Option<&str>) -> Result<()> {
    // Override provider if specified.
    if let Some(p) = provider_override {
        // SAFETY: called before any threads are spawned for this operation.
        unsafe { std::env::set_var("LOTHAL_LLM_PROVIDER", p) };
    }

    let client = LlmClient::from_env()?;
    let file_path = Path::new(path);

    println!("Extracting text from {}...", file_path.display());
    let text = lothal_ai::extract::extract_text_from_pdf(file_path)?;
    println!("  Extracted {} characters", text.len());

    // Try to find the right account — for now, prompt user to provide it or
    // pick from available accounts.
    let sites = lothal_db::site::list_sites(pool).await?;
    let site = sites
        .first()
        .context("No sites configured. Run `lothal init` first.")?;

    let accounts =
        lothal_db::bill::list_utility_accounts_by_site(pool, site.id).await?;

    if accounts.is_empty() {
        anyhow::bail!("No utility accounts configured. Run `lothal init` first.");
    }

    let account_names: Vec<String> = accounts
        .iter()
        .map(|a| format!("{} ({})", a.provider_name, a.utility_type))
        .collect();

    let selection = dialoguer::Select::new()
        .with_prompt("Select utility account")
        .items(&account_names)
        .default(0)
        .interact()?;

    let account = &accounts[selection];

    println!(
        "Parsing with {} ({})...",
        client.provider_name(),
        client.model_name()
    );

    match lothal_ai::extract::parse_bill_with_llm(&text, account.id, &client).await {
        Ok(mut bill) => {
            bill.source_file = Some(path.to_string());
            bill.parse_method = Some("llm".to_string());
            bill.llm_model = Some(client.model_name().to_string());

            println!("\nExtracted bill:");
            println!("  Provider:   {}", account.provider_name);
            println!(
                "  Period:     {} to {}",
                bill.period.range.start, bill.period.range.end
            );
            println!("  Statement:  {}", bill.statement_date);
            println!("  Usage:      {:.1} {}", bill.total_usage, bill.usage_unit);
            println!("  Total:      ${:.2}", bill.total_amount.value());
            println!("  Line items: {}", bill.line_items.len());

            for item in &bill.line_items {
                println!(
                    "    {:<30} {:>10} ${:.2}",
                    item.description,
                    item.category,
                    item.amount.value()
                );
            }

            let confirm = dialoguer::Confirm::new()
                .with_prompt("Save to database?")
                .default(true)
                .interact()?;

            if confirm {
                lothal_db::bill::insert_bill(pool, &bill).await?;
                println!("Bill saved (id: {})", bill.id);
            }
        }
        Err(e) => {
            println!("LLM extraction failed: {e}");
            println!("Falling back to regex parser...");

            let bill = lothal_ingest::bill::parse_bill(file_path, account.id)?;
            println!("\nFallback extracted bill:");
            println!(
                "  Period:     {} to {}",
                bill.period.range.start, bill.period.range.end
            );
            println!("  Usage:      {:.1} {}", bill.total_usage, bill.usage_unit);
            println!("  Total:      ${:.2}", bill.total_amount.value());

            let confirm = dialoguer::Confirm::new()
                .with_prompt("Save to database?")
                .default(true)
                .interact()?;

            if confirm {
                lothal_db::bill::insert_bill(pool, &bill).await?;
                println!("Bill saved (id: {})", bill.id);
            }
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Daily Briefing
// ---------------------------------------------------------------------------

pub async fn briefing(pool: &PgPool, date_str: Option<&str>, output: &str) -> Result<()> {
    let date = match date_str {
        Some(s) => chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
            .context("Date must be in YYYY-MM-DD format")?,
        None => chrono::Local::now().date_naive() - chrono::Duration::days(1),
    };

    let client = LlmClient::from_env()?;
    let invoker: std::sync::Arc<dyn lothal_ontology::llm_function::LlmInvoker> =
        std::sync::Arc::new(lothal_ai::LlmClientInvoker::new(client));
    let functions = lothal_ai::functions::default_registry(invoker);

    let sites = lothal_db::site::list_sites(pool).await?;
    let site = sites
        .first()
        .context("No sites configured. Run `lothal init` first.")?;

    println!("Generating briefing for {} ({})", date, site.address);

    let content =
        lothal_ai::briefing::generate_briefing(pool, site.id, date, &functions).await?;

    // Send to output target.
    if output == "stdout" {
        println!("\n{content}");
    } else {
        // SAFETY: called before any concurrent env reads for this config.
        unsafe { std::env::set_var("BRIEFING_OUTPUT", output) };
        let target = lothal_ai::briefing::format::BriefingOutput::from_env()?;
        target.send(&content).await?;
        println!("Briefing sent to {output}");
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// MCP Server
// ---------------------------------------------------------------------------

pub async fn mcp_server(pool: PgPool) -> Result<()> {
    lothal_ai::mcp::run_server(pool).await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Ingest Email
// ---------------------------------------------------------------------------

pub async fn ingest_email(pool: &PgPool, once: bool) -> Result<()> {
    let client = LlmClient::from_env()?;
    let config = lothal_ai::extract::email::ImapConfig::from_env()?;

    let sites = lothal_db::site::list_sites(pool).await?;
    let site = sites
        .first()
        .context("No sites configured")?;

    let accounts =
        lothal_db::bill::list_utility_accounts_by_site(pool, site.id).await?;
    let account = accounts
        .first()
        .context("No utility accounts configured")?;

    loop {
        println!("Checking for new bill emails...");

        let results =
            lothal_ai::extract::email::poll_and_ingest(&config, pool, account.id, &client)
                .await?;

        for result in &results {
            match &result.status {
                lothal_ai::extract::email::EmailStatus::Parsed { bill_id } => {
                    println!("  Parsed: {} -> bill {bill_id}", result.sender);
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
                    println!("  Skipped: {} ({reason})", result.sender);
                }
                lothal_ai::extract::email::EmailStatus::Failed(error) => {
                    println!("  Failed: {} ({error})", result.sender);
                    lothal_db::ai::insert_email_ingest_log(
                        pool,
                        &result.message_id,
                        &result.sender,
                        result.subject.as_deref(),
                        None,
                        "failed",
                        Some(error),
                    )
                    .await?;
                }
            }
        }

        if once {
            break;
        }

        println!("Sleeping 5 minutes...");
        tokio::time::sleep(std::time::Duration::from_secs(300)).await;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// NILM Identification
// ---------------------------------------------------------------------------

pub async fn identify(pool: &PgPool, circuit: &str, window: &str) -> Result<()> {
    let client = LlmClient::from_env()?;
    let invoker: std::sync::Arc<dyn lothal_ontology::llm_function::LlmInvoker> =
        std::sync::Arc::new(lothal_ai::LlmClientInvoker::new(client));
    let functions = lothal_ai::functions::default_registry(invoker);

    let window_days: u32 = if window.ends_with('d') {
        window.trim_end_matches('d').parse().context("Invalid window")?
    } else {
        window.parse().context("Invalid window (use e.g., '7d')")?
    };

    // Parse circuit: UUID or "all".
    let circuit_ids = if circuit == "all" {
        // Get all circuits.
        let sites = lothal_db::site::list_sites(pool).await?;
        let mut ids = Vec::new();
        for site in &sites {
            let structures = lothal_db::site::get_structures_by_site(pool, site.id).await?;
            for s in &structures {
                let panels = lothal_db::site::get_panels_by_structure(pool, s.id).await?;
                for p in &panels {
                    let circuits = lothal_db::device::get_circuits_by_panel(pool, p.id).await?;
                    ids.extend(circuits.iter().map(|c| (c.id, c.label.clone())));
                }
            }
        }
        ids
    } else {
        let id: Uuid = circuit.parse().context("Invalid circuit UUID")?;
        vec![(id, circuit.to_string())]
    };

    for (cid, label) in &circuit_ids {
        println!("Analyzing circuit '{}' ({cid})...", label);

        let labels =
            lothal_ai::nilm::identify_devices(pool, *cid, window_days, &functions).await?;

        if labels.is_empty() {
            println!("  No power signatures detected");
            continue;
        }

        for lbl in &labels {
            println!(
                "  {} (confidence: {:.0}%) — {}",
                lbl.device_kind,
                lbl.confidence * 100.0,
                lbl.reasoning
            );
        }
    }

    Ok(())
}
