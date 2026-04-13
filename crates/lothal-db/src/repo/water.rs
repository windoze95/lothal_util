use chrono::NaiveDate;
use sqlx::PgPool;
use uuid::Uuid;

use lothal_core::ontology::water::{
    CoverType, Pool, SepticSystem, WaterFlow, WaterSource, WaterSourceKind,
};
use lothal_core::units::{Gallons, SquareFeet, Usd};
use lothal_ontology::indexer;
use lothal_ontology::{Describe, EventSpec, LinkSpec, ObjectRef};

// ---------------------------------------------------------------------------
// WaterSource
// ---------------------------------------------------------------------------

pub async fn insert_water_source(
    pool: &PgPool,
    source: &WaterSource,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"INSERT INTO water_sources
               (id, site_id, name, kind, capacity_gallons, flow_rate_gpm,
                cost_per_gallon, notes, created_at, updated_at)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)"#,
    )
    .bind(source.id)
    .bind(source.site_id)
    .bind(&source.name)
    .bind(source.kind.to_string())
    .bind(source.capacity_gallons.map(|g| g.value()))
    .bind(source.flow_rate_gpm)
    .bind(source.cost_per_gallon.map(|u| u.value()))
    .bind(&source.notes)
    .bind(source.created_at)
    .bind(source.updated_at)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list_water_sources_by_site(
    pool: &PgPool,
    site_id: Uuid,
) -> Result<Vec<WaterSource>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, site_id, name, kind, capacity_gallons, flow_rate_gpm,
                cost_per_gallon, notes, created_at, updated_at
         FROM water_sources WHERE site_id = $1 ORDER BY name",
    )
    .bind(site_id)
    .fetch_all(pool)
    .await?;

    Ok(rows.iter().map(water_source_from_row).collect())
}

fn water_source_from_row(row: &sqlx::postgres::PgRow) -> WaterSource {
    use sqlx::Row;
    let kind_str: String = row.get("kind");
    let cap: Option<f64> = row.get("capacity_gallons");
    let cpg: Option<f64> = row.get("cost_per_gallon");

    WaterSource {
        id: row.get("id"),
        site_id: row.get("site_id"),
        name: row.get("name"),
        kind: kind_str.parse().unwrap_or(WaterSourceKind::Municipal),
        capacity_gallons: cap.map(Gallons::new),
        flow_rate_gpm: row.get("flow_rate_gpm"),
        cost_per_gallon: cpg.map(Usd::new),
        notes: row.get("notes"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

// ---------------------------------------------------------------------------
// Pool
// ---------------------------------------------------------------------------

pub async fn insert_pool(pool: &PgPool, entity: &Pool) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query(
        r#"INSERT INTO pools
               (id, site_id, name, volume_gallons, surface_area_sqft,
                pump_device_id, heater_device_id, cleaner_device_id,
                cover_type, notes, created_at, updated_at)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)"#,
    )
    .bind(entity.id)
    .bind(entity.site_id)
    .bind(&entity.name)
    .bind(entity.volume_gallons.value())
    .bind(entity.surface_area_sqft.map(|s| s.value()))
    .bind(entity.pump_device_id)
    .bind(entity.heater_device_id)
    .bind(entity.cleaner_device_id)
    .bind(entity.cover_type.map(|c| c.to_string()))
    .bind(&entity.notes)
    .bind(entity.created_at)
    .bind(entity.updated_at)
    .execute(&mut *tx)
    .await?;

    indexer::upsert_object(&mut tx, entity).await?;
    indexer::upsert_link(
        &mut tx,
        LinkSpec::new(
            "contained_in",
            ObjectRef::new(Pool::KIND, entity.id),
            ObjectRef::new("site", entity.site_id),
        ),
    )
    .await?;
    indexer::emit_event(
        &mut tx,
        EventSpec::record_registered(entity, "repo:pool"),
    )
    .await?;

    tx.commit().await?;
    Ok(())
}

pub async fn get_pool(pool: &PgPool, id: Uuid) -> Result<Option<Pool>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT id, site_id, name, volume_gallons, surface_area_sqft,
                pump_device_id, heater_device_id, cleaner_device_id,
                cover_type, notes, created_at, updated_at
         FROM pools WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| pool_from_row(&r)))
}

pub async fn list_pools_by_site(
    pool: &PgPool,
    site_id: Uuid,
) -> Result<Vec<Pool>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, site_id, name, volume_gallons, surface_area_sqft,
                pump_device_id, heater_device_id, cleaner_device_id,
                cover_type, notes, created_at, updated_at
         FROM pools WHERE site_id = $1 ORDER BY name",
    )
    .bind(site_id)
    .fetch_all(pool)
    .await?;

    Ok(rows.iter().map(pool_from_row).collect())
}

fn pool_from_row(row: &sqlx::postgres::PgRow) -> Pool {
    use sqlx::Row;
    let vol: f64 = row.get("volume_gallons");
    let area: Option<f64> = row.get("surface_area_sqft");
    let cover: Option<String> = row.get("cover_type");

    Pool {
        id: row.get("id"),
        site_id: row.get("site_id"),
        name: row.get("name"),
        volume_gallons: Gallons::new(vol),
        surface_area_sqft: area.map(SquareFeet::new),
        pump_device_id: row.get("pump_device_id"),
        heater_device_id: row.get("heater_device_id"),
        cleaner_device_id: row.get("cleaner_device_id"),
        cover_type: cover.and_then(|s| s.parse::<CoverType>().ok()),
        notes: row.get("notes"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

// ---------------------------------------------------------------------------
// SepticSystem
// ---------------------------------------------------------------------------

pub async fn insert_septic_system(
    pool: &PgPool,
    septic: &SepticSystem,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"INSERT INTO septic_systems
               (id, site_id, tank_capacity_gallons, leach_field_zone_id,
                last_pumped, pump_interval_months, daily_load_estimate_gallons,
                notes, created_at, updated_at)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)"#,
    )
    .bind(septic.id)
    .bind(septic.site_id)
    .bind(septic.tank_capacity_gallons.map(|g| g.value()))
    .bind(septic.leach_field_zone_id)
    .bind(septic.last_pumped)
    .bind(septic.pump_interval_months)
    .bind(septic.daily_load_estimate_gallons)
    .bind(&septic.notes)
    .bind(septic.created_at)
    .bind(septic.updated_at)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_septic_system(
    pool: &PgPool,
    site_id: Uuid,
) -> Result<Option<SepticSystem>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT id, site_id, tank_capacity_gallons, leach_field_zone_id,
                last_pumped, pump_interval_months, daily_load_estimate_gallons,
                notes, created_at, updated_at
         FROM septic_systems WHERE site_id = $1
         LIMIT 1",
    )
    .bind(site_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| septic_from_row(&r)))
}

fn septic_from_row(row: &sqlx::postgres::PgRow) -> SepticSystem {
    use sqlx::Row;
    let cap: Option<f64> = row.get("tank_capacity_gallons");
    let last: Option<NaiveDate> = row.get("last_pumped");

    SepticSystem {
        id: row.get("id"),
        site_id: row.get("site_id"),
        tank_capacity_gallons: cap.map(Gallons::new),
        leach_field_zone_id: row.get("leach_field_zone_id"),
        last_pumped: last,
        pump_interval_months: row.get("pump_interval_months"),
        daily_load_estimate_gallons: row.get("daily_load_estimate_gallons"),
        notes: row.get("notes"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

// ---------------------------------------------------------------------------
// WaterFlow
// ---------------------------------------------------------------------------

pub async fn insert_water_flow(
    pool: &PgPool,
    flow: &WaterFlow,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"INSERT INTO water_flows
               (id, site_id, name, source_type, source_id, sink_type, sink_id,
                flow_rate_gpm, is_active, notes, created_at)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)"#,
    )
    .bind(flow.id)
    .bind(flow.site_id)
    .bind(&flow.name)
    .bind(&flow.source_type)
    .bind(flow.source_id)
    .bind(&flow.sink_type)
    .bind(flow.sink_id)
    .bind(flow.flow_rate_gpm)
    .bind(flow.is_active)
    .bind(&flow.notes)
    .bind(flow.created_at)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list_water_flows_by_site(
    pool: &PgPool,
    site_id: Uuid,
) -> Result<Vec<WaterFlow>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, site_id, name, source_type, source_id, sink_type, sink_id,
                flow_rate_gpm, is_active, notes, created_at
         FROM water_flows WHERE site_id = $1 ORDER BY name",
    )
    .bind(site_id)
    .fetch_all(pool)
    .await?;

    Ok(rows.iter().map(water_flow_from_row).collect())
}

fn water_flow_from_row(row: &sqlx::postgres::PgRow) -> WaterFlow {
    use sqlx::Row;
    WaterFlow {
        id: row.get("id"),
        site_id: row.get("site_id"),
        name: row.get("name"),
        source_type: row.get("source_type"),
        source_id: row.get("source_id"),
        sink_type: row.get("sink_type"),
        sink_id: row.get("sink_id"),
        flow_rate_gpm: row.get("flow_rate_gpm"),
        is_active: row.get("is_active"),
        notes: row.get("notes"),
        created_at: row.get("created_at"),
    }
}
