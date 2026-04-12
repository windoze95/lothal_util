use dialoguer::{Input, Select};
use sqlx::PgPool;

use lothal_core::ontology::water::*;
use lothal_core::units::Gallons;

pub async fn list_sources(pool: &PgPool) -> anyhow::Result<()> {
    let sites = lothal_db::site::list_sites(pool).await?;
    let site = sites.first().ok_or_else(|| anyhow::anyhow!("No sites found."))?;

    let sources = lothal_db::water::list_water_sources_by_site(pool, site.id).await?;
    let pools = lothal_db::water::list_pools_by_site(pool, site.id).await?;
    let septic = lothal_db::water::get_septic_system(pool, site.id).await?;

    println!("=== Water Sources ===");
    if sources.is_empty() {
        println!("  (none)");
    }
    for s in &sources {
        let cap = s.capacity_gallons.map(|g| format!(" ({:.0} gal)", g.value())).unwrap_or_default();
        println!("  {} ({}){cap}", s.name, s.kind);
    }

    if !pools.is_empty() {
        println!("\n=== Pools ===");
        for p in &pools {
            let cover = p.cover_type.map(|c| format!(", cover: {c}")).unwrap_or_default();
            println!("  {} - {:.0} gal{cover}", p.name, p.volume_gallons.value());
        }
    }

    if let Some(sep) = septic {
        println!("\n=== Septic System ===");
        let cap = sep.tank_capacity_gallons.map(|g| format!("{:.0} gal", g.value())).unwrap_or("unknown".into());
        let next = sep.estimated_next_pump().map(|d| d.to_string()).unwrap_or("unknown".into());
        println!("  Tank: {cap}");
        println!("  Last pumped: {}", sep.last_pumped.map(|d| d.to_string()).unwrap_or("unknown".into()));
        println!("  Next pump due: {next}");
    }

    Ok(())
}

pub async fn add_source(pool: &PgPool) -> anyhow::Result<()> {
    let sites = lothal_db::site::list_sites(pool).await?;
    let site = sites.first().ok_or_else(|| anyhow::anyhow!("No sites found."))?;

    let name: String = Input::new().with_prompt("Source name").interact_text()?;

    let kind_opts = ["municipal", "well", "cistern", "rainwater_collection"];
    let kind_idx = Select::new()
        .with_prompt("Source type")
        .items(&kind_opts)
        .default(0)
        .interact()?;
    let kind: WaterSourceKind = kind_opts[kind_idx].parse().map_err(|e: String| anyhow::anyhow!(e))?;

    let source = WaterSource::new(site.id, name.clone(), kind);
    lothal_db::water::insert_water_source(pool, &source).await?;
    println!("Added water source: {name} ({kind})");
    Ok(())
}

pub async fn add_pool(pool: &PgPool) -> anyhow::Result<()> {
    let sites = lothal_db::site::list_sites(pool).await?;
    let site = sites.first().ok_or_else(|| anyhow::anyhow!("No sites found."))?;

    let name: String = Input::new().with_prompt("Pool name").interact_text()?;
    let volume: f64 = Input::new().with_prompt("Volume (gallons)").interact_text()?;

    let pool_entity = Pool::new(site.id, name.clone(), Gallons::new(volume));
    lothal_db::water::insert_pool(pool, &pool_entity).await?;
    println!("Added pool: {name} ({volume:.0} gal)");
    Ok(())
}

pub async fn add_septic(pool: &PgPool) -> anyhow::Result<()> {
    let sites = lothal_db::site::list_sites(pool).await?;
    let site = sites.first().ok_or_else(|| anyhow::anyhow!("No sites found."))?;

    let cap_str: String = Input::new()
        .with_prompt("Tank capacity (gallons, blank if unknown)")
        .default(String::new())
        .interact_text()?;

    let interval_str: String = Input::new()
        .with_prompt("Pump interval (months, typically 36-60)")
        .default("36".to_string())
        .interact_text()?;

    let last_str: String = Input::new()
        .with_prompt("Last pumped (YYYY-MM-DD, blank if unknown)")
        .default(String::new())
        .interact_text()?;

    let mut septic = SepticSystem::new(site.id);
    septic.tank_capacity_gallons = cap_str.parse::<f64>().ok().map(Gallons::new);
    septic.pump_interval_months = interval_str.parse().ok();
    septic.last_pumped = last_str.parse().ok();

    lothal_db::water::insert_septic_system(pool, &septic).await?;
    println!("Added septic system.");
    if let Some(next) = septic.estimated_next_pump() {
        println!("  Next pump estimated: {next}");
    }
    Ok(())
}
