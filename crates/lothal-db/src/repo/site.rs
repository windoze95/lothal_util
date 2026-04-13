use sqlx::PgPool;
use uuid::Uuid;

use lothal_core::ontology::site::{FoundationType, Site, SoilType, Structure, Zone};
use lothal_core::ontology::circuit::Panel;
use lothal_core::units::{Acres, SquareFeet};
use lothal_ontology::indexer;
use lothal_ontology::{Describe, EventSpec, LinkSpec, ObjectRef};

// ---------------------------------------------------------------------------
// Site
// ---------------------------------------------------------------------------

pub async fn insert_site(pool: &PgPool, site: &Site) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query(
        r#"INSERT INTO sites (id, address, city, state, zip, latitude, longitude,
                              lot_size, climate_zone, soil_type, created_at, updated_at)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)"#,
    )
    .bind(site.id)
    .bind(&site.address)
    .bind(&site.city)
    .bind(&site.state)
    .bind(&site.zip)
    .bind(site.latitude)
    .bind(site.longitude)
    .bind(site.lot_size.value())
    .bind(&site.climate_zone)
    .bind(site.soil_type.map(|s| s.to_string()))
    .bind(site.created_at)
    .bind(site.updated_at)
    .execute(&mut *tx)
    .await?;

    indexer::upsert_object(&mut tx, site).await?;
    indexer::emit_event(&mut tx, EventSpec::record_registered(site, "repo:site")).await?;

    tx.commit().await?;
    Ok(())
}

pub async fn get_site(pool: &PgPool, id: Uuid) -> Result<Option<Site>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT id, address, city, state, zip, latitude, longitude,
                lot_size, climate_zone, soil_type, created_at, updated_at
         FROM sites WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| site_from_row(&r)))
}

pub async fn list_sites(pool: &PgPool) -> Result<Vec<Site>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, address, city, state, zip, latitude, longitude,
                lot_size, climate_zone, soil_type, created_at, updated_at
         FROM sites ORDER BY created_at",
    )
    .fetch_all(pool)
    .await?;

    Ok(rows.iter().map(site_from_row).collect())
}

pub async fn update_site_boundary(
    pool: &PgPool,
    site_id: Uuid,
    boundary: serde_json::Value,
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query("UPDATE sites SET boundary = $2 WHERE id = $1")
        .bind(site_id)
        .bind(boundary)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

pub async fn update_site(pool: &PgPool, site: &Site) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query(
        r#"UPDATE sites SET address = $2, city = $3, state = $4, zip = $5,
                            latitude = $6, longitude = $7, lot_size = $8,
                            climate_zone = $9, soil_type = $10, updated_at = $11
           WHERE id = $1"#,
    )
    .bind(site.id)
    .bind(&site.address)
    .bind(&site.city)
    .bind(&site.state)
    .bind(&site.zip)
    .bind(site.latitude)
    .bind(site.longitude)
    .bind(site.lot_size.value())
    .bind(&site.climate_zone)
    .bind(site.soil_type.map(|s| s.to_string()))
    .bind(site.updated_at)
    .execute(&mut *tx)
    .await?;

    indexer::upsert_object(&mut tx, site).await?;

    tx.commit().await?;
    Ok(())
}

fn site_from_row(row: &sqlx::postgres::PgRow) -> Site {
    use sqlx::Row;
    let soil_str: Option<String> = row.get("soil_type");
    Site {
        id: row.get("id"),
        address: row.get("address"),
        city: row.get("city"),
        state: row.get("state"),
        zip: row.get("zip"),
        latitude: row.get("latitude"),
        longitude: row.get("longitude"),
        lot_size: Acres::new(row.get("lot_size")),
        climate_zone: row.get("climate_zone"),
        soil_type: soil_str.and_then(|s| s.parse::<SoilType>().ok()),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

// ---------------------------------------------------------------------------
// Structure
// ---------------------------------------------------------------------------

pub async fn insert_structure(pool: &PgPool, structure: &Structure) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query(
        r#"INSERT INTO structures (id, site_id, name, year_built, square_footage, stories,
                                   foundation_type, has_pool, pool_gallons, has_septic,
                                   created_at, updated_at)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)"#,
    )
    .bind(structure.id)
    .bind(structure.site_id)
    .bind(&structure.name)
    .bind(structure.year_built)
    .bind(structure.square_footage.value())
    .bind(structure.stories)
    .bind(structure.foundation_type.map(|f| f.to_string()))
    .bind(structure.has_pool)
    .bind(structure.pool_gallons)
    .bind(structure.has_septic)
    .bind(structure.created_at)
    .bind(structure.updated_at)
    .execute(&mut *tx)
    .await?;

    indexer::upsert_object(&mut tx, structure).await?;
    indexer::upsert_link(
        &mut tx,
        LinkSpec::new(
            "contained_in",
            ObjectRef::new(Structure::KIND, structure.id),
            ObjectRef::new("site", structure.site_id),
        ),
    )
    .await?;
    indexer::emit_event(
        &mut tx,
        EventSpec::record_registered(structure, "repo:structure"),
    )
    .await?;

    tx.commit().await?;
    Ok(())
}

pub async fn update_structure_footprint(
    pool: &PgPool,
    structure_id: Uuid,
    footprint: serde_json::Value,
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query("UPDATE structures SET footprint = $2 WHERE id = $1")
        .bind(structure_id)
        .bind(footprint)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

pub async fn get_structures_by_site(
    pool: &PgPool,
    site_id: Uuid,
) -> Result<Vec<Structure>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, site_id, name, year_built, square_footage, stories,
                foundation_type, has_pool, pool_gallons, has_septic,
                created_at, updated_at
         FROM structures WHERE site_id = $1 ORDER BY name",
    )
    .bind(site_id)
    .fetch_all(pool)
    .await?;

    Ok(rows.iter().map(structure_from_row).collect())
}

fn structure_from_row(row: &sqlx::postgres::PgRow) -> Structure {
    use sqlx::Row;
    let foundation_str: Option<String> = row.get("foundation_type");
    Structure {
        id: row.get("id"),
        site_id: row.get("site_id"),
        name: row.get("name"),
        year_built: row.get("year_built"),
        square_footage: SquareFeet::new(row.get("square_footage")),
        stories: row.get("stories"),
        foundation_type: foundation_str.and_then(|s| s.parse::<FoundationType>().ok()),
        has_pool: row.get("has_pool"),
        pool_gallons: row.get("pool_gallons"),
        has_septic: row.get("has_septic"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

// ---------------------------------------------------------------------------
// Zone
// ---------------------------------------------------------------------------

pub async fn insert_zone(pool: &PgPool, zone: &Zone) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"INSERT INTO zones (id, structure_id, name, floor, square_footage, created_at, updated_at)
           VALUES ($1, $2, $3, $4, $5, $6, $7)"#,
    )
    .bind(zone.id)
    .bind(zone.structure_id)
    .bind(&zone.name)
    .bind(zone.floor)
    .bind(zone.square_footage.map(|sf| sf.value()))
    .bind(zone.created_at)
    .bind(zone.updated_at)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_zones_by_structure(
    pool: &PgPool,
    structure_id: Uuid,
) -> Result<Vec<Zone>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, structure_id, name, floor, square_footage, created_at, updated_at
         FROM zones WHERE structure_id = $1 ORDER BY name",
    )
    .bind(structure_id)
    .fetch_all(pool)
    .await?;

    Ok(rows.iter().map(zone_from_row).collect())
}

fn zone_from_row(row: &sqlx::postgres::PgRow) -> Zone {
    use sqlx::Row;
    let sqft: Option<f64> = row.get("square_footage");
    Zone {
        id: row.get("id"),
        structure_id: row.get("structure_id"),
        name: row.get("name"),
        floor: row.get("floor"),
        square_footage: sqft.map(SquareFeet::new),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

// ---------------------------------------------------------------------------
// Panel
// ---------------------------------------------------------------------------

pub async fn insert_panel(pool: &PgPool, panel: &Panel) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"INSERT INTO panels (id, structure_id, name, amperage, is_main, created_at, updated_at)
           VALUES ($1, $2, $3, $4, $5, $6, $7)"#,
    )
    .bind(panel.id)
    .bind(panel.structure_id)
    .bind(&panel.name)
    .bind(panel.amperage)
    .bind(panel.is_main)
    .bind(panel.created_at)
    .bind(panel.updated_at)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_panels_by_structure(
    pool: &PgPool,
    structure_id: Uuid,
) -> Result<Vec<Panel>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, structure_id, name, amperage, is_main, created_at, updated_at
         FROM panels WHERE structure_id = $1 ORDER BY name",
    )
    .bind(structure_id)
    .fetch_all(pool)
    .await?;

    Ok(rows.iter().map(panel_from_row).collect())
}

fn panel_from_row(row: &sqlx::postgres::PgRow) -> Panel {
    use sqlx::Row;
    Panel {
        id: row.get("id"),
        structure_id: row.get("structure_id"),
        name: row.get("name"),
        amperage: row.get("amperage"),
        is_main: row.get("is_main"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}
