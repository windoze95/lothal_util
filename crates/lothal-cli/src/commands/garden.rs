use chrono::NaiveDate;
use dialoguer::{Input, Select};
use sqlx::PgPool;

use lothal_core::ontology::garden::*;
use lothal_core::units::SquareFeet;

pub async fn list_beds(pool: &PgPool) -> anyhow::Result<()> {
    let sites = lothal_db::site::list_sites(pool).await?;
    let site = sites.first().ok_or_else(|| anyhow::anyhow!("No sites found."))?;

    let beds = lothal_db::garden::list_garden_beds_by_site(pool, site.id).await?;
    let compost = lothal_db::garden::list_compost_piles_by_site(pool, site.id).await?;

    if beds.is_empty() && compost.is_empty() {
        println!("No garden beds or compost piles. Use `lothal garden add-bed` to start.");
        return Ok(());
    }

    if !beds.is_empty() {
        println!("=== Garden Beds ===");
        for bed in &beds {
            let area = bed.area_sqft.map(|a| format!(" ({:.0} sqft)", a.value())).unwrap_or_default();
            println!("  {} ({}){area}", bed.name, bed.bed_type);
        }
    }

    if !compost.is_empty() {
        println!("\n=== Compost Piles ===");
        for pile in &compost {
            let fill = pile.fill_pct().map(|p| format!(" - {p:.0}% full")).unwrap_or_default();
            println!("  {}{fill}", pile.name);
        }
    }

    Ok(())
}

pub async fn add_bed(pool: &PgPool) -> anyhow::Result<()> {
    let sites = lothal_db::site::list_sites(pool).await?;
    let site = sites.first().ok_or_else(|| anyhow::anyhow!("No sites found."))?;

    let name: String = Input::new().with_prompt("Bed name").interact_text()?;

    let type_opts = ["in_ground", "raised", "container", "hydroponic", "other"];
    let type_idx = Select::new()
        .with_prompt("Bed type")
        .items(&type_opts)
        .default(1)
        .interact()?;
    let bed_type: BedType = type_opts[type_idx].parse().map_err(|e: String| anyhow::anyhow!(e))?;

    let area_str: String = Input::new()
        .with_prompt("Area (sqft, blank to skip)")
        .default(String::new())
        .interact_text()?;

    let mut bed = GardenBed::new(site.id, name.clone(), bed_type);
    bed.area_sqft = area_str.parse::<f64>().ok().map(SquareFeet::new);

    lothal_db::garden::insert_garden_bed(pool, &bed).await?;
    println!("Added garden bed: {name} ({bed_type})");
    Ok(())
}

pub async fn add_planting(pool: &PgPool) -> anyhow::Result<()> {
    let sites = lothal_db::site::list_sites(pool).await?;
    let site = sites.first().ok_or_else(|| anyhow::anyhow!("No sites found."))?;

    let beds = lothal_db::garden::list_garden_beds_by_site(pool, site.id).await?;
    if beds.is_empty() {
        anyhow::bail!("No garden beds found. Add a bed first.");
    }

    let bed_names: Vec<&str> = beds.iter().map(|b| b.name.as_str()).collect();
    let bed_idx = Select::new()
        .with_prompt("Select bed")
        .items(&bed_names)
        .interact()?;
    let bed = &beds[bed_idx];

    let crop: String = Input::new().with_prompt("Crop").interact_text()?;
    let variety: String = Input::new()
        .with_prompt("Variety (blank to skip)")
        .default(String::new())
        .interact_text()?;

    let today = chrono::Utc::now().date_naive();
    let date_str: String = Input::new()
        .with_prompt("Date planted (YYYY-MM-DD)")
        .default(today.to_string())
        .interact_text()?;
    let date: NaiveDate = date_str.parse()?;

    let mut planting = Planting::new(bed.id, crop.clone(), date);
    planting.variety = if variety.is_empty() { None } else { Some(variety) };

    lothal_db::garden::insert_planting(pool, &planting).await?;
    println!("Planted {crop} in {} on {date}", bed.name);
    Ok(())
}

pub async fn add_compost_pile(pool: &PgPool) -> anyhow::Result<()> {
    let sites = lothal_db::site::list_sites(pool).await?;
    let site = sites.first().ok_or_else(|| anyhow::anyhow!("No sites found."))?;

    let name: String = Input::new().with_prompt("Pile name").interact_text()?;

    let pile = CompostPile::new(site.id, name.clone());
    lothal_db::garden::insert_compost_pile(pool, &pile).await?;
    println!("Added compost pile: {name}");
    Ok(())
}
