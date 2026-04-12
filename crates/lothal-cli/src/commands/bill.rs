use std::path::Path;

use anyhow::{bail, Context, Result};
use chrono::NaiveDate;
use comfy_table::{Cell, Table};
use dialoguer::{Confirm, Input, Select};
use sqlx::PgPool;

use lothal_core::ontology::bill::{Bill, BillLineItem, LineItemCategory};
use lothal_core::units::Usd;

const LINE_ITEM_CATEGORY_LABELS: &[&str] = &[
    "Base Charge",
    "Energy Charge",
    "Delivery Charge",
    "Fuel Cost Adjustment",
    "Demand Charge",
    "Rider Charge",
    "Tax",
    "Fee",
    "Credit",
    "Other",
];

const LINE_ITEM_CATEGORY_VALUES: &[LineItemCategory] = &[
    LineItemCategory::BaseCharge,
    LineItemCategory::EnergyCharge,
    LineItemCategory::DeliveryCharge,
    LineItemCategory::FuelCostAdjustment,
    LineItemCategory::DemandCharge,
    LineItemCategory::RiderCharge,
    LineItemCategory::Tax,
    LineItemCategory::Fee,
    LineItemCategory::Credit,
    LineItemCategory::Other,
];

/// Select a utility account interactively.
async fn select_account(pool: &PgPool) -> Result<lothal_core::UtilityAccount> {
    let sites = lothal_db::site::list_sites(pool).await?;
    let site = sites
        .first()
        .ok_or_else(|| anyhow::anyhow!("No site found. Run `lothal init` first."))?;

    let accounts =
        lothal_db::bill::list_utility_accounts_by_site(pool, site.id).await?;
    if accounts.is_empty() {
        bail!("No utility accounts found. Run `lothal init` to set them up.");
    }

    if accounts.len() == 1 {
        println!("Using account: {} ({})", accounts[0].provider_name, accounts[0].utility_type);
        return Ok(accounts.into_iter().next().unwrap());
    }

    let labels: Vec<String> = accounts
        .iter()
        .map(|a| {
            format!(
                "{} ({}){}",
                a.provider_name,
                a.utility_type,
                a.account_number
                    .as_deref()
                    .map(|n| format!(" [{}]", n))
                    .unwrap_or_default()
            )
        })
        .collect();

    let idx = Select::new()
        .with_prompt("Select utility account")
        .items(&labels)
        .default(0)
        .interact()
        .context("failed to select account")?;

    Ok(accounts.into_iter().nth(idx).unwrap())
}

/// Parse a date from user input in YYYY-MM-DD format.
fn parse_date(prompt: &str) -> Result<NaiveDate> {
    let s: String = Input::new()
        .with_prompt(prompt)
        .interact_text()
        .context("failed to read date")?;
    NaiveDate::parse_from_str(&s, "%Y-%m-%d")
        .with_context(|| format!("invalid date '{}' -- expected YYYY-MM-DD", s))
}

/// Interactive bill entry.
pub async fn add_bill(pool: &PgPool) -> Result<()> {
    println!();
    println!("--- Add Utility Bill ---");
    println!();

    let account = select_account(pool).await?;
    println!();

    let period_start = parse_date("Period start (YYYY-MM-DD)")?;
    let period_end = parse_date("Period end (YYYY-MM-DD)")?;
    let statement_date = parse_date("Statement date (YYYY-MM-DD)")?;

    let total_usage: f64 = Input::new()
        .with_prompt("Total usage")
        .interact_text()
        .context("failed to read usage")?;

    let usage_unit: String = Input::new()
        .with_prompt("Usage unit (e.g., kWh, therms, gallons)")
        .default(default_usage_unit(&account.utility_type))
        .interact_text()
        .context("failed to read usage unit")?;

    let total_amount: f64 = Input::new()
        .with_prompt("Total amount ($)")
        .interact_text()
        .context("failed to read amount")?;

    let mut bill = Bill::new(
        account.id,
        period_start,
        period_end,
        statement_date,
        total_usage,
        usage_unit,
        Usd::new(total_amount),
    );

    // Line items
    println!();
    println!("Add line items (press Enter with empty description to finish):");
    println!();

    loop {
        let description: String = Input::new()
            .with_prompt("  Line item description (or blank to finish)")
            .default(String::new())
            .interact_text()
            .context("failed to read description")?;

        if description.is_empty() {
            break;
        }

        let cat_idx = Select::new()
            .with_prompt("  Category")
            .items(LINE_ITEM_CATEGORY_LABELS)
            .default(0)
            .interact()
            .context("failed to read category")?;

        let amount: f64 = Input::new()
            .with_prompt("  Amount ($)")
            .interact_text()
            .context("failed to read line item amount")?;

        let item = BillLineItem::new(
            bill.id,
            description,
            LINE_ITEM_CATEGORY_VALUES[cat_idx],
            Usd::new(amount),
        );
        bill.line_items.push(item);
    }

    // Show summary before saving
    println!();
    display_bill_summary(&bill);

    // Validate line items if any were entered
    if !bill.line_items.is_empty() {
        match bill.validate_line_items() {
            lothal_core::LineItemValidation::Valid => {
                println!("  Line items sum matches total.");
            }
            lothal_core::LineItemValidation::Mismatch {
                expected,
                actual,
                difference,
            } => {
                println!(
                    "  WARNING: Line items sum to ${:.2} but total is ${:.2} (diff: ${:.2})",
                    actual.value(),
                    expected.value(),
                    difference.value()
                );
            }
        }
        println!();
    }

    let confirmed = Confirm::new()
        .with_prompt("Save this bill?")
        .default(true)
        .interact()
        .context("failed to read confirmation")?;

    if !confirmed {
        println!("Cancelled.");
        return Ok(());
    }

    lothal_db::bill::insert_bill(pool, &bill).await?;
    println!("Bill saved: {} (id: {})", bill.period.range, bill.id);
    Ok(())
}

/// Import a bill from a file (PDF, CSV, or XML). Auto-detects by extension.
pub async fn import_bill(pool: &PgPool, path: &str) -> Result<()> {
    let file_path = Path::new(path);
    if !file_path.exists() {
        bail!("File not found: {}", path);
    }

    let ext = file_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    println!();
    println!("Importing bill from: {}", path);
    println!("Detected format: {}", match ext.as_str() {
        "pdf" => "PDF",
        "csv" => "CSV",
        "xml" => "XML (Green Button)",
        _ => "unknown",
    });
    println!();

    let account = select_account(pool).await?;

    let bill = match ext.as_str() {
        "pdf" => lothal_ingest::bill::parse_bill(file_path, account.id)?,
        _ => {
            bail!(
                "Unsupported file format: .{}. Currently supported: PDF.",
                ext
            );
        }
    };

    println!();
    display_bill_summary(&bill);

    if !bill.line_items.is_empty() {
        println!("  Line items:");
        let mut items_table = Table::new();
        items_table.set_header(vec!["Description", "Category", "Amount"]);
        for item in &bill.line_items {
            items_table.add_row(vec![
                Cell::new(&item.description),
                Cell::new(item.category.to_string()),
                Cell::new(format!("${:.2}", item.amount.value())),
            ]);
        }
        println!("{items_table}");
        println!();
    }

    let confirmed = Confirm::new()
        .with_prompt("Save this bill?")
        .default(true)
        .interact()
        .context("failed to read confirmation")?;

    if !confirmed {
        println!("Cancelled.");
        return Ok(());
    }

    lothal_db::bill::insert_bill(pool, &bill).await?;
    println!("Bill imported and saved (id: {})", bill.id);
    Ok(())
}

/// List bills, optionally filtered by account.
pub async fn list_bills(pool: &PgPool, account_filter: Option<&str>) -> Result<()> {
    let sites = lothal_db::site::list_sites(pool).await?;
    let site = sites
        .first()
        .ok_or_else(|| anyhow::anyhow!("No site found. Run `lothal init` first."))?;

    let accounts =
        lothal_db::bill::list_utility_accounts_by_site(pool, site.id).await?;

    if accounts.is_empty() {
        println!("No utility accounts found.");
        return Ok(());
    }

    // Filter accounts if a filter was provided
    let target_accounts: Vec<_> = if let Some(filter) = account_filter {
        let lower = filter.to_lowercase();
        accounts
            .into_iter()
            .filter(|a| {
                a.provider_name.to_lowercase().contains(&lower)
                    || a.utility_type.to_string().to_lowercase().contains(&lower)
                    || a.account_number
                        .as_deref()
                        .map(|n| n.to_lowercase().contains(&lower))
                        .unwrap_or(false)
                    || a.id.to_string().starts_with(&lower)
            })
            .collect()
    } else {
        accounts
    };

    if target_accounts.is_empty() {
        println!("No matching utility accounts.");
        return Ok(());
    }

    for account in &target_accounts {
        println!();
        println!(
            "=== {} ({}) ===",
            account.provider_name, account.utility_type
        );

        let bills =
            lothal_db::bill::list_bills_by_account(pool, account.id).await?;

        if bills.is_empty() {
            println!("  No bills on file.");
            continue;
        }

        let mut table = Table::new();
        table.set_header(vec![
            "Period",
            "Days",
            "Usage",
            "Amount",
            "Eff. Rate",
            "Daily Cost",
        ]);

        for bill in &bills {
            let period_str = format!(
                "{} to {}",
                bill.period.range.start, bill.period.range.end
            );
            let days = bill.period.days();
            let usage_str = format!("{:.1} {}", bill.total_usage, bill.usage_unit);
            let amount_str = format!("${:.2}", bill.total_amount.value());
            let rate_str = bill
                .effective_rate()
                .map(|r| format!("${:.4}/{}", r.value(), bill.usage_unit))
                .unwrap_or_else(|| "-".into());
            let daily_cost_str = bill
                .daily_cost()
                .map(|c| format!("${:.2}/day", c.value()))
                .unwrap_or_else(|| "-".into());

            table.add_row(vec![
                Cell::new(&period_str),
                Cell::new(days),
                Cell::new(&usage_str),
                Cell::new(&amount_str),
                Cell::new(&rate_str),
                Cell::new(&daily_cost_str),
            ]);
        }

        println!("{table}");

        // Summary row
        let total_amount: f64 = bills.iter().map(|b| b.total_amount.value()).sum();
        let total_usage: f64 = bills.iter().map(|b| b.total_usage).sum();
        println!(
            "  {} bills | Total: ${:.2} | Total usage: {:.1}",
            bills.len(),
            total_amount,
            total_usage
        );
    }

    println!();
    Ok(())
}

/// Display a formatted bill summary.
fn display_bill_summary(bill: &Bill) {
    println!("  Bill Summary:");
    println!("    Period:     {} to {}", bill.period.range.start, bill.period.range.end);
    println!("    Days:       {}", bill.period.days());
    println!("    Statement:  {}", bill.statement_date);
    println!("    Usage:      {:.2} {}", bill.total_usage, bill.usage_unit);
    println!("    Amount:     ${:.2}", bill.total_amount.value());
    if let Some(rate) = bill.effective_rate() {
        println!("    Eff. rate:  ${:.4}/{}", rate.value(), bill.usage_unit);
    }
    if let Some(daily) = bill.daily_cost() {
        println!("    Daily cost: ${:.2}/day", daily.value());
    }
    println!();
}

/// Return the default usage unit for a given utility type.
fn default_usage_unit(ut: &lothal_core::UtilityType) -> String {
    match ut {
        lothal_core::UtilityType::Electric => "kWh".into(),
        lothal_core::UtilityType::Gas => "therms".into(),
        lothal_core::UtilityType::Water | lothal_core::UtilityType::Sewer => "gallons".into(),
        lothal_core::UtilityType::Propane => "gallons".into(),
        _ => "units".into(),
    }
}
