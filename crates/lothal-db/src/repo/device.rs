use sqlx::PgPool;
use uuid::Uuid;

use lothal_core::ontology::circuit::Circuit;
use lothal_core::ontology::device::{Device, DeviceKind};
use lothal_core::units::{Usd, Watts};
use lothal_ontology::indexer;
use lothal_ontology::{Describe, EventSpec, LinkSpec, ObjectRef};

// ---------------------------------------------------------------------------
// Device
// ---------------------------------------------------------------------------

pub async fn insert_device(pool: &PgPool, device: &Device) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query(
        r#"INSERT INTO devices (id, structure_id, zone_id, circuit_id, name, kind,
                                make, model, nameplate_watts, estimated_daily_hours,
                                year_installed, expected_lifespan_years, replacement_cost,
                                notes, created_at, updated_at)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)"#,
    )
    .bind(device.id)
    .bind(device.structure_id)
    .bind(device.zone_id)
    .bind(device.circuit_id)
    .bind(&device.name)
    .bind(device.kind.to_string())
    .bind(&device.make)
    .bind(&device.model)
    .bind(device.nameplate_watts.map(|w| w.value()))
    .bind(device.estimated_daily_hours)
    .bind(device.year_installed)
    .bind(device.expected_lifespan_years)
    .bind(device.replacement_cost.map(|c| c.value()))
    .bind(&device.notes)
    .bind(device.created_at)
    .bind(device.updated_at)
    .execute(&mut *tx)
    .await?;

    indexer::upsert_object(&mut tx, device).await?;
    indexer::upsert_link(
        &mut tx,
        LinkSpec::new(
            "contained_in",
            ObjectRef::new(Device::KIND, device.id),
            ObjectRef::new("structure", device.structure_id),
        ),
    )
    .await?;
    indexer::emit_event(
        &mut tx,
        EventSpec::record_registered(device, "repo:device"),
    )
    .await?;

    tx.commit().await?;
    Ok(())
}

pub async fn get_device(pool: &PgPool, id: Uuid) -> Result<Option<Device>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT id, structure_id, zone_id, circuit_id, name, kind,
                make, model, nameplate_watts, estimated_daily_hours,
                year_installed, expected_lifespan_years, replacement_cost,
                notes, created_at, updated_at
         FROM devices WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| device_from_row(&r)))
}

pub async fn list_devices_by_structure(
    pool: &PgPool,
    structure_id: Uuid,
) -> Result<Vec<Device>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, structure_id, zone_id, circuit_id, name, kind,
                make, model, nameplate_watts, estimated_daily_hours,
                year_installed, expected_lifespan_years, replacement_cost,
                notes, created_at, updated_at
         FROM devices WHERE structure_id = $1 ORDER BY name",
    )
    .bind(structure_id)
    .fetch_all(pool)
    .await?;

    Ok(rows.iter().map(device_from_row).collect())
}

fn device_from_row(row: &sqlx::postgres::PgRow) -> Device {
    use sqlx::Row;
    let kind_str: String = row.get("kind");
    let nameplate: Option<f64> = row.get("nameplate_watts");
    let cost: Option<f64> = row.get("replacement_cost");
    Device {
        id: row.get("id"),
        structure_id: row.get("structure_id"),
        zone_id: row.get("zone_id"),
        circuit_id: row.get("circuit_id"),
        name: row.get("name"),
        kind: kind_str.parse::<DeviceKind>().unwrap_or(DeviceKind::Other),
        make: row.get("make"),
        model: row.get("model"),
        nameplate_watts: nameplate.map(Watts::new),
        estimated_daily_hours: row.get("estimated_daily_hours"),
        year_installed: row.get("year_installed"),
        expected_lifespan_years: row.get("expected_lifespan_years"),
        replacement_cost: cost.map(Usd::new),
        notes: row.get("notes"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

// ---------------------------------------------------------------------------
// Circuit
// ---------------------------------------------------------------------------

pub async fn insert_circuit(pool: &PgPool, circuit: &Circuit) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query(
        r#"INSERT INTO circuits (id, panel_id, breaker_number, label, amperage,
                                 is_double_pole, device_id, created_at, updated_at)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"#,
    )
    .bind(circuit.id)
    .bind(circuit.panel_id)
    .bind(circuit.breaker_number)
    .bind(&circuit.label)
    .bind(circuit.amperage)
    .bind(circuit.is_double_pole)
    .bind(circuit.device_id)
    .bind(circuit.created_at)
    .bind(circuit.updated_at)
    .execute(&mut *tx)
    .await?;

    indexer::upsert_object(&mut tx, circuit).await?;
    // Circuit's only FK is `panel_id`; `Panel` has no `Describe` impl, so we
    // do not emit a `contained_in` link here. If a circuit is bound to a
    // device, link `powers` from circuit to device to capture the electrical
    // relationship.
    if let Some(device_id) = circuit.device_id {
        indexer::upsert_link(
            &mut tx,
            LinkSpec::new(
                "powers",
                ObjectRef::new(Circuit::KIND, circuit.id),
                ObjectRef::new("device", device_id),
            ),
        )
        .await?;
    }
    indexer::emit_event(
        &mut tx,
        EventSpec::record_registered(circuit, "repo:circuit"),
    )
    .await?;

    tx.commit().await?;
    Ok(())
}

pub async fn get_circuits_by_panel(
    pool: &PgPool,
    panel_id: Uuid,
) -> Result<Vec<Circuit>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, panel_id, breaker_number, label, amperage,
                is_double_pole, device_id, created_at, updated_at
         FROM circuits WHERE panel_id = $1 ORDER BY breaker_number",
    )
    .bind(panel_id)
    .fetch_all(pool)
    .await?;

    Ok(rows.iter().map(circuit_from_row).collect())
}

fn circuit_from_row(row: &sqlx::postgres::PgRow) -> Circuit {
    use sqlx::Row;
    Circuit {
        id: row.get("id"),
        panel_id: row.get("panel_id"),
        breaker_number: row.get("breaker_number"),
        label: row.get("label"),
        amperage: row.get("amperage"),
        is_double_pole: row.get("is_double_pole"),
        device_id: row.get("device_id"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}
