use sqlx::PgPool;
use uuid::Uuid;

use lothal_core::ontology::property_zone::{
    Constraint, ConstraintKind, DrainageType, PropertyZone, PropertyZoneKind, Slope, SunExposure,
};
use lothal_core::ontology::site::SoilType;
use lothal_core::ontology::tree::{Tree, TreeHealth};
use lothal_core::units::{SquareFeet, Usd};

// ---------------------------------------------------------------------------
// PropertyZone
// ---------------------------------------------------------------------------

pub async fn insert_property_zone(
    pool: &PgPool,
    zone: &PropertyZone,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"INSERT INTO property_zones
               (id, site_id, name, kind, area_sqft, sun_exposure, slope,
                soil_type, drainage, notes, created_at, updated_at)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)"#,
    )
    .bind(zone.id)
    .bind(zone.site_id)
    .bind(&zone.name)
    .bind(zone.kind.to_string())
    .bind(zone.area_sqft.map(|a| a.value()))
    .bind(zone.sun_exposure.map(|e| e.to_string()))
    .bind(zone.slope.map(|s| s.to_string()))
    .bind(zone.soil_type.map(|s| s.to_string()))
    .bind(zone.drainage.map(|d| d.to_string()))
    .bind(&zone.notes)
    .bind(zone.created_at)
    .bind(zone.updated_at)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_property_zone(
    pool: &PgPool,
    id: Uuid,
) -> Result<Option<PropertyZone>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT id, site_id, name, kind, area_sqft, sun_exposure, slope,
                soil_type, drainage, notes, created_at, updated_at
         FROM property_zones WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| property_zone_from_row(&r)))
}

pub async fn list_property_zones_by_site(
    pool: &PgPool,
    site_id: Uuid,
) -> Result<Vec<PropertyZone>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, site_id, name, kind, area_sqft, sun_exposure, slope,
                soil_type, drainage, notes, created_at, updated_at
         FROM property_zones WHERE site_id = $1 ORDER BY name",
    )
    .bind(site_id)
    .fetch_all(pool)
    .await?;

    Ok(rows.iter().map(property_zone_from_row).collect())
}

pub async fn update_property_zone(
    pool: &PgPool,
    zone: &PropertyZone,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"UPDATE property_zones SET
               name = $2, kind = $3, area_sqft = $4, sun_exposure = $5,
               slope = $6, soil_type = $7, drainage = $8, notes = $9,
               updated_at = $10
           WHERE id = $1"#,
    )
    .bind(zone.id)
    .bind(&zone.name)
    .bind(zone.kind.to_string())
    .bind(zone.area_sqft.map(|a| a.value()))
    .bind(zone.sun_exposure.map(|e| e.to_string()))
    .bind(zone.slope.map(|s| s.to_string()))
    .bind(zone.soil_type.map(|s| s.to_string()))
    .bind(zone.drainage.map(|d| d.to_string()))
    .bind(&zone.notes)
    .bind(zone.updated_at)
    .execute(pool)
    .await?;
    Ok(())
}

fn property_zone_from_row(row: &sqlx::postgres::PgRow) -> PropertyZone {
    use sqlx::Row;
    let kind_str: String = row.get("kind");
    let area: Option<f64> = row.get("area_sqft");
    let sun: Option<String> = row.get("sun_exposure");
    let slope: Option<String> = row.get("slope");
    let soil: Option<String> = row.get("soil_type");
    let drain: Option<String> = row.get("drainage");

    PropertyZone {
        id: row.get("id"),
        site_id: row.get("site_id"),
        name: row.get("name"),
        kind: kind_str.parse().unwrap_or(PropertyZoneKind::Unstructured),
        area_sqft: area.map(SquareFeet::new),
        sun_exposure: sun.and_then(|s| s.parse::<SunExposure>().ok()),
        slope: slope.and_then(|s| s.parse::<Slope>().ok()),
        soil_type: soil.and_then(|s| s.parse::<SoilType>().ok()),
        drainage: drain.and_then(|s| s.parse::<DrainageType>().ok()),
        notes: row.get("notes"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

// ---------------------------------------------------------------------------
// Constraint
// ---------------------------------------------------------------------------

pub async fn insert_constraint(
    pool: &PgPool,
    constraint: &Constraint,
    affected_zone_ids: &[Uuid],
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;

    sqlx::query(
        r#"INSERT INTO constraints (id, site_id, kind, description, geometry, created_at)
           VALUES ($1, $2, $3, $4, $5, $6)"#,
    )
    .bind(constraint.id)
    .bind(constraint.site_id)
    .bind(constraint.kind.to_string())
    .bind(&constraint.description)
    .bind(&constraint.geometry)
    .bind(constraint.created_at)
    .execute(&mut *tx)
    .await?;

    for zone_id in affected_zone_ids {
        sqlx::query(
            "INSERT INTO constraint_zones (constraint_id, zone_id) VALUES ($1, $2)",
        )
        .bind(constraint.id)
        .bind(zone_id)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(())
}

pub async fn list_constraints_by_site(
    pool: &PgPool,
    site_id: Uuid,
) -> Result<Vec<Constraint>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, site_id, kind, description, geometry, created_at
         FROM constraints WHERE site_id = $1 ORDER BY kind",
    )
    .bind(site_id)
    .fetch_all(pool)
    .await?;

    Ok(rows.iter().map(constraint_from_row).collect())
}

pub async fn get_constraint_zone_ids(
    pool: &PgPool,
    constraint_id: Uuid,
) -> Result<Vec<Uuid>, sqlx::Error> {
    let rows = sqlx::query_as::<_, (Uuid,)>(
        "SELECT zone_id FROM constraint_zones WHERE constraint_id = $1",
    )
    .bind(constraint_id)
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(|(id,)| id).collect())
}

fn constraint_from_row(row: &sqlx::postgres::PgRow) -> Constraint {
    use sqlx::Row;
    let kind_str: String = row.get("kind");

    Constraint {
        id: row.get("id"),
        site_id: row.get("site_id"),
        kind: kind_str.parse().unwrap_or(ConstraintKind::Other),
        description: row.get("description"),
        geometry: row.get("geometry"),
        created_at: row.get("created_at"),
    }
}

// ---------------------------------------------------------------------------
// Tree
// ---------------------------------------------------------------------------

pub async fn insert_tree(pool: &PgPool, tree: &Tree) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"INSERT INTO trees
               (id, site_id, property_zone_id, species, common_name,
                canopy_radius_ft, height_ft, health, distance_to_structure_ft,
                shade_direction, estimated_cooling_value_usd, notes,
                created_at, updated_at)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)"#,
    )
    .bind(tree.id)
    .bind(tree.site_id)
    .bind(tree.property_zone_id)
    .bind(&tree.species)
    .bind(&tree.common_name)
    .bind(tree.canopy_radius_ft)
    .bind(tree.height_ft)
    .bind(tree.health.to_string())
    .bind(tree.distance_to_structure_ft)
    .bind(&tree.shade_direction)
    .bind(tree.estimated_cooling_value_usd.map(|u| u.value()))
    .bind(&tree.notes)
    .bind(tree.created_at)
    .bind(tree.updated_at)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_tree(pool: &PgPool, id: Uuid) -> Result<Option<Tree>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT id, site_id, property_zone_id, species, common_name,
                canopy_radius_ft, height_ft, health, distance_to_structure_ft,
                shade_direction, estimated_cooling_value_usd, notes,
                created_at, updated_at
         FROM trees WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| tree_from_row(&r)))
}

pub async fn list_trees_by_site(
    pool: &PgPool,
    site_id: Uuid,
) -> Result<Vec<Tree>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, site_id, property_zone_id, species, common_name,
                canopy_radius_ft, height_ft, health, distance_to_structure_ft,
                shade_direction, estimated_cooling_value_usd, notes,
                created_at, updated_at
         FROM trees WHERE site_id = $1 ORDER BY species",
    )
    .bind(site_id)
    .fetch_all(pool)
    .await?;

    Ok(rows.iter().map(tree_from_row).collect())
}

pub async fn list_trees_by_zone(
    pool: &PgPool,
    zone_id: Uuid,
) -> Result<Vec<Tree>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, site_id, property_zone_id, species, common_name,
                canopy_radius_ft, height_ft, health, distance_to_structure_ft,
                shade_direction, estimated_cooling_value_usd, notes,
                created_at, updated_at
         FROM trees WHERE property_zone_id = $1 ORDER BY species",
    )
    .bind(zone_id)
    .fetch_all(pool)
    .await?;

    Ok(rows.iter().map(tree_from_row).collect())
}

fn tree_from_row(row: &sqlx::postgres::PgRow) -> Tree {
    use sqlx::Row;
    let health_str: String = row.get("health");
    let cooling: Option<f64> = row.get("estimated_cooling_value_usd");

    Tree {
        id: row.get("id"),
        site_id: row.get("site_id"),
        property_zone_id: row.get("property_zone_id"),
        species: row.get("species"),
        common_name: row.get("common_name"),
        canopy_radius_ft: row.get("canopy_radius_ft"),
        height_ft: row.get("height_ft"),
        health: health_str.parse().unwrap_or(TreeHealth::Unknown),
        distance_to_structure_ft: row.get("distance_to_structure_ft"),
        shade_direction: row.get("shade_direction"),
        estimated_cooling_value_usd: cooling.map(Usd::new),
        notes: row.get("notes"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}
