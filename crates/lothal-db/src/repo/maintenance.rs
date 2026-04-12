use sqlx::PgPool;
use uuid::Uuid;

use lothal_core::ontology::maintenance::{MaintenanceEvent, MaintenanceTarget, MaintenanceType};
use lothal_core::units::Usd;

// ---------------------------------------------------------------------------
// MaintenanceEvent
// ---------------------------------------------------------------------------

pub async fn insert_maintenance_event(
    pool: &PgPool,
    event: &MaintenanceEvent,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"INSERT INTO maintenance_events
               (id, target_type, target_id, date, event_type, description,
                cost, provider, next_due, notes, created_at)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)"#,
    )
    .bind(event.id)
    .bind(event.target.target_type())
    .bind(event.target.target_id())
    .bind(event.date)
    .bind(event.event_type.to_string())
    .bind(&event.description)
    .bind(event.cost.map(|c| c.value()))
    .bind(&event.provider)
    .bind(event.next_due)
    .bind(&event.notes)
    .bind(event.created_at)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list_maintenance_by_target(
    pool: &PgPool,
    target_type: &str,
    target_id: Uuid,
) -> Result<Vec<MaintenanceEvent>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, target_type, target_id, date, event_type, description,
                cost, provider, next_due, notes, created_at
         FROM maintenance_events
         WHERE target_type = $1 AND target_id = $2
         ORDER BY date DESC",
    )
    .bind(target_type)
    .bind(target_id)
    .fetch_all(pool)
    .await?;

    Ok(rows.iter().map(maintenance_from_row).collect())
}

pub async fn get_upcoming_maintenance(
    pool: &PgPool,
    site_id: Uuid,
) -> Result<Vec<MaintenanceEvent>, sqlx::Error> {
    // Join against devices/structures to scope to a site.
    // next_due must be non-null and in the future.
    let rows = sqlx::query(
        r#"SELECT m.id, m.target_type, m.target_id, m.date, m.event_type,
                  m.description, m.cost, m.provider, m.next_due, m.notes, m.created_at
           FROM maintenance_events m
           WHERE m.next_due IS NOT NULL AND m.next_due >= CURRENT_DATE
             AND (
               (m.target_type = 'device' AND m.target_id IN (
                   SELECT d.id FROM devices d
                   JOIN structures s ON d.structure_id = s.id
                   WHERE s.site_id = $1
               ))
               OR
               (m.target_type = 'structure' AND m.target_id IN (
                   SELECT s.id FROM structures s WHERE s.site_id = $1
               ))
             )
           ORDER BY m.next_due"#,
    )
    .bind(site_id)
    .fetch_all(pool)
    .await?;

    Ok(rows.iter().map(maintenance_from_row).collect())
}

fn maintenance_from_row(row: &sqlx::postgres::PgRow) -> MaintenanceEvent {
    use sqlx::Row;
    let target_type: String = row.get("target_type");
    let target_id: Uuid = row.get("target_id");
    let event_type_str: String = row.get("event_type");
    let cost_val: Option<f64> = row.get("cost");

    let target = match target_type.as_str() {
        "device" => MaintenanceTarget::Device(target_id),
        "structure" => MaintenanceTarget::Structure(target_id),
        _ => MaintenanceTarget::Device(target_id), // fallback
    };

    let event_type = parse_maintenance_type(&event_type_str);

    MaintenanceEvent {
        id: row.get("id"),
        target,
        date: row.get("date"),
        event_type,
        description: row.get("description"),
        cost: cost_val.map(Usd::new),
        provider: row.get("provider"),
        next_due: row.get("next_due"),
        notes: row.get("notes"),
        created_at: row.get("created_at"),
    }
}

fn parse_maintenance_type(s: &str) -> MaintenanceType {
    match s.to_lowercase().as_str() {
        "inspection" => MaintenanceType::Inspection,
        "repair" => MaintenanceType::Repair,
        "replacement" => MaintenanceType::Replacement,
        "cleaning" => MaintenanceType::Cleaning,
        "filter change" => MaintenanceType::FilterChange,
        "tune-up" | "tune" => MaintenanceType::Tune,
        "septic pump" => MaintenanceType::SepticPump,
        "pool service" => MaintenanceType::PoolService,
        "pest control" => MaintenanceType::PestControl,
        _ => MaintenanceType::Other,
    }
}
