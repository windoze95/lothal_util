use comfy_table::{presets::UTF8_FULL, Table};
use dialoguer::{Input, Select};
use sqlx::PgPool;

use lothal_core::ontology::property_zone::*;
use lothal_core::units::SquareFeet;

pub async fn list_zones(pool: &PgPool) -> anyhow::Result<()> {
    let sites = lothal_db::site::list_sites(pool).await?;
    let site = sites.first().ok_or_else(|| anyhow::anyhow!("No sites found. Run `lothal init` first."))?;

    let zones = lothal_db::property_zone::list_property_zones_by_site(pool, site.id).await?;
    let constraints = lothal_db::property_zone::list_constraints_by_site(pool, site.id).await?;

    if zones.is_empty() {
        println!("No property zones defined. Use `lothal property add-zone` to add one.");
        return Ok(());
    }

    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_header(vec!["Name", "Kind", "Area", "Sun", "Drainage"]);

    for zone in &zones {
        table.add_row(vec![
            zone.name.clone(),
            zone.kind.to_string(),
            zone.area_sqft.map(|a| format!("{:.0} sqft", a.value())).unwrap_or_default(),
            zone.sun_exposure.map(|e| e.to_string()).unwrap_or_default(),
            zone.drainage.map(|d| d.to_string()).unwrap_or_default(),
        ]);
    }
    println!("{table}");

    if !constraints.is_empty() {
        println!("\nConstraints:");
        for c in &constraints {
            println!("  [{}] {}", c.kind, c.description);
        }
    }

    Ok(())
}

pub async fn add_zone(pool: &PgPool) -> anyhow::Result<()> {
    let sites = lothal_db::site::list_sites(pool).await?;
    let site = sites.first().ok_or_else(|| anyhow::anyhow!("No sites found."))?;

    let name: String = Input::new().with_prompt("Zone name").interact_text()?;

    let kinds = [
        "lawn", "garden", "orchard", "tree_line", "driveway", "pool_deck",
        "leach_field", "coop_area", "run", "pasture", "patio", "storage", "unstructured",
    ];
    let kind_idx = Select::new()
        .with_prompt("Zone kind")
        .items(&kinds)
        .default(0)
        .interact()?;
    let kind: PropertyZoneKind = kinds[kind_idx].parse().map_err(|e: String| anyhow::anyhow!(e))?;

    let area_str: String = Input::new()
        .with_prompt("Area (sqft, blank to skip)")
        .default(String::new())
        .interact_text()?;
    let area = area_str.parse::<f64>().ok().map(SquareFeet::new);

    let mut zone = PropertyZone::new(site.id, name, kind);
    zone.area_sqft = area;

    lothal_db::property_zone::insert_property_zone(pool, &zone).await?;
    println!("Added property zone: {} ({})", zone.name, zone.kind);
    Ok(())
}

pub async fn add_constraint(pool: &PgPool) -> anyhow::Result<()> {
    let sites = lothal_db::site::list_sites(pool).await?;
    let site = sites.first().ok_or_else(|| anyhow::anyhow!("No sites found."))?;

    let kind_opts = ["leach_field", "easement", "setback", "utility_line", "wellhead", "flood_zone", "other"];
    let kind_idx = Select::new()
        .with_prompt("Constraint type")
        .items(&kind_opts)
        .default(0)
        .interact()?;
    let kind: ConstraintKind = kind_opts[kind_idx].parse().map_err(|e: String| anyhow::anyhow!(e))?;

    let description: String = Input::new().with_prompt("Description").interact_text()?;

    let constraint = Constraint::new(site.id, kind, description);
    lothal_db::property_zone::insert_constraint(pool, &constraint, &[]).await?;
    println!("Added constraint: {} - {}", constraint.kind, constraint.description);
    Ok(())
}
