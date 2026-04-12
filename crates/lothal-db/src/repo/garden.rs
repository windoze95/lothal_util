use sqlx::PgPool;
use uuid::Uuid;

use lothal_core::ontology::garden::{BedType, CompostPile, GardenBed, Planting};
use lothal_core::units::{CubicFeet, Gallons, Pounds, SquareFeet};

// ---------------------------------------------------------------------------
// GardenBed
// ---------------------------------------------------------------------------

pub async fn insert_garden_bed(pool: &PgPool, bed: &GardenBed) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"INSERT INTO garden_beds
               (id, site_id, property_zone_id, name, area_sqft, bed_type,
                soil_amendments, irrigation_source_id, created_at, updated_at)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)"#,
    )
    .bind(bed.id)
    .bind(bed.site_id)
    .bind(bed.property_zone_id)
    .bind(&bed.name)
    .bind(bed.area_sqft.map(|a| a.value()))
    .bind(bed.bed_type.to_string())
    .bind(&bed.soil_amendments)
    .bind(bed.irrigation_source_id)
    .bind(bed.created_at)
    .bind(bed.updated_at)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list_garden_beds_by_site(
    pool: &PgPool,
    site_id: Uuid,
) -> Result<Vec<GardenBed>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, site_id, property_zone_id, name, area_sqft, bed_type,
                soil_amendments, irrigation_source_id, created_at, updated_at
         FROM garden_beds WHERE site_id = $1 ORDER BY name",
    )
    .bind(site_id)
    .fetch_all(pool)
    .await?;

    Ok(rows.iter().map(garden_bed_from_row).collect())
}

fn garden_bed_from_row(row: &sqlx::postgres::PgRow) -> GardenBed {
    use sqlx::Row;
    let area: Option<f64> = row.get("area_sqft");
    let type_str: String = row.get("bed_type");

    GardenBed {
        id: row.get("id"),
        site_id: row.get("site_id"),
        property_zone_id: row.get("property_zone_id"),
        name: row.get("name"),
        area_sqft: area.map(SquareFeet::new),
        bed_type: type_str.parse().unwrap_or(BedType::Other),
        soil_amendments: row.get("soil_amendments"),
        irrigation_source_id: row.get("irrigation_source_id"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

// ---------------------------------------------------------------------------
// Planting
// ---------------------------------------------------------------------------

pub async fn insert_planting(pool: &PgPool, planting: &Planting) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"INSERT INTO plantings
               (id, bed_id, crop, variety, date_planted, date_harvested,
                yield_lbs, water_consumed_gallons, notes, created_at)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)"#,
    )
    .bind(planting.id)
    .bind(planting.bed_id)
    .bind(&planting.crop)
    .bind(&planting.variety)
    .bind(planting.date_planted)
    .bind(planting.date_harvested)
    .bind(planting.yield_lbs.map(|p| p.value()))
    .bind(planting.water_consumed_gallons.map(|g| g.value()))
    .bind(&planting.notes)
    .bind(planting.created_at)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list_plantings_by_bed(
    pool: &PgPool,
    bed_id: Uuid,
) -> Result<Vec<Planting>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, bed_id, crop, variety, date_planted, date_harvested,
                yield_lbs, water_consumed_gallons, notes, created_at
         FROM plantings WHERE bed_id = $1 ORDER BY date_planted DESC",
    )
    .bind(bed_id)
    .fetch_all(pool)
    .await?;

    Ok(rows.iter().map(planting_from_row).collect())
}

pub async fn list_plantings_by_season(
    pool: &PgPool,
    site_id: Uuid,
    year: i32,
) -> Result<Vec<Planting>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT p.id, p.bed_id, p.crop, p.variety, p.date_planted, p.date_harvested,
                p.yield_lbs, p.water_consumed_gallons, p.notes, p.created_at
         FROM plantings p
         JOIN garden_beds gb ON p.bed_id = gb.id
         WHERE gb.site_id = $1
           AND EXTRACT(YEAR FROM p.date_planted) = $2
         ORDER BY p.date_planted",
    )
    .bind(site_id)
    .bind(year)
    .fetch_all(pool)
    .await?;

    Ok(rows.iter().map(planting_from_row).collect())
}

fn planting_from_row(row: &sqlx::postgres::PgRow) -> Planting {
    use sqlx::Row;
    let yield_val: Option<f64> = row.get("yield_lbs");
    let water_val: Option<f64> = row.get("water_consumed_gallons");

    Planting {
        id: row.get("id"),
        bed_id: row.get("bed_id"),
        crop: row.get("crop"),
        variety: row.get("variety"),
        date_planted: row.get("date_planted"),
        date_harvested: row.get("date_harvested"),
        yield_lbs: yield_val.map(Pounds::new),
        water_consumed_gallons: water_val.map(Gallons::new),
        notes: row.get("notes"),
        created_at: row.get("created_at"),
    }
}

// ---------------------------------------------------------------------------
// CompostPile
// ---------------------------------------------------------------------------

pub async fn insert_compost_pile(
    pool: &PgPool,
    pile: &CompostPile,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"INSERT INTO compost_piles
               (id, site_id, property_zone_id, name, capacity_cuft,
                current_volume_cuft, notes, created_at, updated_at)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"#,
    )
    .bind(pile.id)
    .bind(pile.site_id)
    .bind(pile.property_zone_id)
    .bind(&pile.name)
    .bind(pile.capacity_cuft.map(|c| c.value()))
    .bind(pile.current_volume_cuft.map(|c| c.value()))
    .bind(&pile.notes)
    .bind(pile.created_at)
    .bind(pile.updated_at)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list_compost_piles_by_site(
    pool: &PgPool,
    site_id: Uuid,
) -> Result<Vec<CompostPile>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, site_id, property_zone_id, name, capacity_cuft,
                current_volume_cuft, notes, created_at, updated_at
         FROM compost_piles WHERE site_id = $1 ORDER BY name",
    )
    .bind(site_id)
    .fetch_all(pool)
    .await?;

    Ok(rows.iter().map(compost_from_row).collect())
}

pub async fn update_compost_pile(
    pool: &PgPool,
    pile: &CompostPile,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"UPDATE compost_piles SET
               current_volume_cuft = $2, notes = $3, updated_at = $4
           WHERE id = $1"#,
    )
    .bind(pile.id)
    .bind(pile.current_volume_cuft.map(|c| c.value()))
    .bind(&pile.notes)
    .bind(pile.updated_at)
    .execute(pool)
    .await?;
    Ok(())
}

fn compost_from_row(row: &sqlx::postgres::PgRow) -> CompostPile {
    use sqlx::Row;
    let cap: Option<f64> = row.get("capacity_cuft");
    let cur: Option<f64> = row.get("current_volume_cuft");

    CompostPile {
        id: row.get("id"),
        site_id: row.get("site_id"),
        property_zone_id: row.get("property_zone_id"),
        name: row.get("name"),
        capacity_cuft: cap.map(CubicFeet::new),
        current_volume_cuft: cur.map(CubicFeet::new),
        notes: row.get("notes"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}
