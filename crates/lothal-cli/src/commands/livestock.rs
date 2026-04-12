use chrono::NaiveDate;
use dialoguer::{Input, Select};
use sqlx::PgPool;

use lothal_core::ontology::livestock::*;

pub async fn add_flock(pool: &PgPool) -> anyhow::Result<()> {
    let sites = lothal_db::site::list_sites(pool).await?;
    let site = sites.first().ok_or_else(|| anyhow::anyhow!("No sites found."))?;

    let name: String = Input::new().with_prompt("Flock name").interact_text()?;
    let breed: String = Input::new().with_prompt("Breed").interact_text()?;
    let count: i32 = Input::new().with_prompt("Number of birds").interact_text()?;

    let flock = Flock::new(site.id, name.clone(), breed, count);
    lothal_db::livestock::insert_flock(pool, &flock).await?;
    println!("Added flock: {name} ({count} birds)");
    Ok(())
}

pub async fn show_flock(pool: &PgPool) -> anyhow::Result<()> {
    let sites = lothal_db::site::list_sites(pool).await?;
    let site = sites.first().ok_or_else(|| anyhow::anyhow!("No sites found."))?;

    let flocks = lothal_db::livestock::list_flocks_by_site(pool, site.id).await?;
    if flocks.is_empty() {
        println!("No flocks registered. Use `lothal livestock add-flock` to add one.");
        return Ok(());
    }

    for flock in &flocks {
        println!("=== {} ===", flock.name);
        println!("  Breed:  {}", flock.breed);
        println!("  Birds:  {}", flock.bird_count);
        println!("  Status: {}", flock.status);
        if let Some(est) = flock.date_established {
            println!("  Established: {est}");
        }

        let paddocks = lothal_db::livestock::list_paddocks_by_flock(pool, flock.id).await?;
        if !paddocks.is_empty() {
            println!("  Paddocks:");
            for p in &paddocks {
                let rest = p.last_rested.map(|d| format!(" (rested {d})")).unwrap_or_default();
                println!("    #{}{rest}", p.rotation_order);
            }
        }
    }
    Ok(())
}

pub async fn log_event(pool: &PgPool) -> anyhow::Result<()> {
    let sites = lothal_db::site::list_sites(pool).await?;
    let site = sites.first().ok_or_else(|| anyhow::anyhow!("No sites found."))?;

    let flocks = lothal_db::livestock::list_flocks_by_site(pool, site.id).await?;
    if flocks.is_empty() {
        anyhow::bail!("No flocks found.");
    }

    let flock_names: Vec<&str> = flocks.iter().map(|f| f.name.as_str()).collect();
    let flock_idx = if flocks.len() == 1 {
        0
    } else {
        Select::new().with_prompt("Select flock").items(&flock_names).interact()?
    };
    let flock = &flocks[flock_idx];

    let event_opts = [
        "egg_collection", "feed_consumed", "water_consumed", "manure_output",
        "predator_incident", "mortality", "paddock_rotation", "vet_visit", "other",
    ];
    let event_idx = Select::new()
        .with_prompt("Event type")
        .items(&event_opts)
        .default(0)
        .interact()?;
    let event_kind: LivestockEventKind = event_opts[event_idx].parse().map_err(|e: String| anyhow::anyhow!(e))?;

    let today = chrono::Utc::now().date_naive();
    let date_str: String = Input::new()
        .with_prompt("Date (YYYY-MM-DD)")
        .default(today.to_string())
        .interact_text()?;
    let date: NaiveDate = date_str.parse()?;

    let qty_str: String = Input::new()
        .with_prompt("Quantity (blank to skip)")
        .default(String::new())
        .interact_text()?;

    let mut log = LivestockLog::new(flock.id, date, event_kind);
    log.quantity = qty_str.parse().ok();

    if log.quantity.is_some() {
        let unit: String = Input::new()
            .with_prompt("Unit (eggs, lbs, gallons, etc.)")
            .default(String::new())
            .interact_text()?;
        log.unit = if unit.is_empty() { None } else { Some(unit) };
    }

    lothal_db::livestock::insert_livestock_log(pool, &log).await?;
    println!("Logged: {} on {date}", event_kind);
    Ok(())
}

pub async fn list_logs(pool: &PgPool, last: &str) -> anyhow::Result<()> {
    let sites = lothal_db::site::list_sites(pool).await?;
    let site = sites.first().ok_or_else(|| anyhow::anyhow!("No sites found."))?;

    let flocks = lothal_db::livestock::list_flocks_by_site(pool, site.id).await?;
    if flocks.is_empty() {
        println!("No flocks found.");
        return Ok(());
    }

    let days: i64 = last
        .trim_end_matches('d')
        .parse()
        .unwrap_or(7);
    let end = chrono::Utc::now().date_naive() + chrono::Duration::days(1);
    let start = end - chrono::Duration::days(days);

    for flock in &flocks {
        let logs = lothal_db::livestock::list_logs_by_date_range(pool, flock.id, start, end).await?;
        if logs.is_empty() {
            continue;
        }
        println!("=== {} (last {days} days) ===", flock.name);
        for log in &logs {
            let qty = log.quantity.map(|q| format!(" {q:.1}")).unwrap_or_default();
            let unit = log.unit.as_deref().unwrap_or("");
            println!("  {} {}{qty} {unit}", log.date, log.event_kind);
        }
    }
    Ok(())
}
