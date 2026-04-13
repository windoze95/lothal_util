use chrono::NaiveDate;
use sqlx::PgPool;
use uuid::Uuid;

use lothal_core::ontology::livestock::{
    Flock, FlockStatus, LivestockEventKind, LivestockLog, Paddock,
};
use lothal_core::units::Usd;
use lothal_ontology::indexer;
use lothal_ontology::{Describe, EventSpec, LinkSpec, ObjectRef};

// ---------------------------------------------------------------------------
// Flock
// ---------------------------------------------------------------------------

pub async fn insert_flock(pool: &PgPool, flock: &Flock) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query(
        r#"INSERT INTO flocks
               (id, site_id, name, breed, bird_count, coop_zone_id,
                date_established, status, notes, created_at, updated_at)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)"#,
    )
    .bind(flock.id)
    .bind(flock.site_id)
    .bind(&flock.name)
    .bind(&flock.breed)
    .bind(flock.bird_count)
    .bind(flock.coop_zone_id)
    .bind(flock.date_established)
    .bind(flock.status.to_string())
    .bind(&flock.notes)
    .bind(flock.created_at)
    .bind(flock.updated_at)
    .execute(&mut *tx)
    .await?;

    indexer::upsert_object(&mut tx, flock).await?;
    indexer::upsert_link(
        &mut tx,
        LinkSpec::new(
            "contained_in",
            ObjectRef::new(Flock::KIND, flock.id),
            ObjectRef::new("site", flock.site_id),
        ),
    )
    .await?;
    indexer::emit_event(
        &mut tx,
        EventSpec::record_registered(flock, "repo:flock"),
    )
    .await?;

    tx.commit().await?;
    Ok(())
}

pub async fn get_flock(pool: &PgPool, id: Uuid) -> Result<Option<Flock>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT id, site_id, name, breed, bird_count, coop_zone_id,
                date_established, status, notes, created_at, updated_at
         FROM flocks WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| flock_from_row(&r)))
}

pub async fn list_flocks_by_site(
    pool: &PgPool,
    site_id: Uuid,
) -> Result<Vec<Flock>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, site_id, name, breed, bird_count, coop_zone_id,
                date_established, status, notes, created_at, updated_at
         FROM flocks WHERE site_id = $1 ORDER BY name",
    )
    .bind(site_id)
    .fetch_all(pool)
    .await?;

    Ok(rows.iter().map(flock_from_row).collect())
}

pub async fn update_flock(pool: &PgPool, flock: &Flock) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query(
        r#"UPDATE flocks SET
               name = $2, breed = $3, bird_count = $4, coop_zone_id = $5,
               status = $6, notes = $7, updated_at = $8
           WHERE id = $1"#,
    )
    .bind(flock.id)
    .bind(&flock.name)
    .bind(&flock.breed)
    .bind(flock.bird_count)
    .bind(flock.coop_zone_id)
    .bind(flock.status.to_string())
    .bind(&flock.notes)
    .bind(flock.updated_at)
    .execute(&mut *tx)
    .await?;

    indexer::upsert_object(&mut tx, flock).await?;

    tx.commit().await?;
    Ok(())
}

fn flock_from_row(row: &sqlx::postgres::PgRow) -> Flock {
    use sqlx::Row;
    let status_str: String = row.get("status");

    Flock {
        id: row.get("id"),
        site_id: row.get("site_id"),
        name: row.get("name"),
        breed: row.get("breed"),
        bird_count: row.get("bird_count"),
        coop_zone_id: row.get("coop_zone_id"),
        date_established: row.get("date_established"),
        status: status_str.parse().unwrap_or(FlockStatus::Active),
        notes: row.get("notes"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

// ---------------------------------------------------------------------------
// Paddock
// ---------------------------------------------------------------------------

pub async fn insert_paddock(pool: &PgPool, paddock: &Paddock) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"INSERT INTO paddocks
               (id, flock_id, property_zone_id, rotation_order,
                last_rested, rest_days_target, created_at, updated_at)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"#,
    )
    .bind(paddock.id)
    .bind(paddock.flock_id)
    .bind(paddock.property_zone_id)
    .bind(paddock.rotation_order)
    .bind(paddock.last_rested)
    .bind(paddock.rest_days_target)
    .bind(paddock.created_at)
    .bind(paddock.updated_at)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list_paddocks_by_flock(
    pool: &PgPool,
    flock_id: Uuid,
) -> Result<Vec<Paddock>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, flock_id, property_zone_id, rotation_order,
                last_rested, rest_days_target, created_at, updated_at
         FROM paddocks WHERE flock_id = $1 ORDER BY rotation_order",
    )
    .bind(flock_id)
    .fetch_all(pool)
    .await?;

    Ok(rows.iter().map(paddock_from_row).collect())
}

fn paddock_from_row(row: &sqlx::postgres::PgRow) -> Paddock {
    use sqlx::Row;
    Paddock {
        id: row.get("id"),
        flock_id: row.get("flock_id"),
        property_zone_id: row.get("property_zone_id"),
        rotation_order: row.get("rotation_order"),
        last_rested: row.get("last_rested"),
        rest_days_target: row.get("rest_days_target"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

// ---------------------------------------------------------------------------
// LivestockLog
// ---------------------------------------------------------------------------

pub async fn insert_livestock_log(
    pool: &PgPool,
    log: &LivestockLog,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"INSERT INTO livestock_logs
               (id, flock_id, date, event_kind, quantity, unit, cost, notes, created_at)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"#,
    )
    .bind(log.id)
    .bind(log.flock_id)
    .bind(log.date)
    .bind(log.event_kind.to_string())
    .bind(log.quantity)
    .bind(&log.unit)
    .bind(log.cost.map(|c| c.value()))
    .bind(&log.notes)
    .bind(log.created_at)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list_logs_by_flock(
    pool: &PgPool,
    flock_id: Uuid,
) -> Result<Vec<LivestockLog>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, flock_id, date, event_kind, quantity, unit, cost, notes, created_at
         FROM livestock_logs WHERE flock_id = $1 ORDER BY date DESC, created_at DESC",
    )
    .bind(flock_id)
    .fetch_all(pool)
    .await?;

    Ok(rows.iter().map(log_from_row).collect())
}

pub async fn list_logs_by_date_range(
    pool: &PgPool,
    flock_id: Uuid,
    start: NaiveDate,
    end: NaiveDate,
) -> Result<Vec<LivestockLog>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, flock_id, date, event_kind, quantity, unit, cost, notes, created_at
         FROM livestock_logs
         WHERE flock_id = $1 AND date >= $2 AND date < $3
         ORDER BY date, created_at",
    )
    .bind(flock_id)
    .bind(start)
    .bind(end)
    .fetch_all(pool)
    .await?;

    Ok(rows.iter().map(log_from_row).collect())
}

/// Aggregate daily summary for a flock on a given date.
pub async fn get_flock_daily_summary(
    pool: &PgPool,
    flock_id: Uuid,
    date: NaiveDate,
) -> Result<FlockDaySummary, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT event_kind, COALESCE(SUM(quantity), 0) as total
         FROM livestock_logs
         WHERE flock_id = $1 AND date = $2
         GROUP BY event_kind",
    )
    .bind(flock_id)
    .bind(date)
    .fetch_all(pool)
    .await?;

    let mut summary = FlockDaySummary::default();
    for row in &rows {
        use sqlx::Row;
        let kind_str: String = row.get("event_kind");
        let total: f64 = row.get("total");
        match kind_str.as_str() {
            "egg_collection" => summary.eggs = total,
            "feed_consumed" => summary.feed_lbs = total,
            "water_consumed" => summary.water_gallons = total,
            "manure_output" => summary.manure_lbs = total,
            "mortality" => summary.mortality = total as i32,
            _ => {}
        }
    }
    Ok(summary)
}

#[derive(Debug, Clone, Default)]
pub struct FlockDaySummary {
    pub eggs: f64,
    pub feed_lbs: f64,
    pub water_gallons: f64,
    pub manure_lbs: f64,
    pub mortality: i32,
}

fn log_from_row(row: &sqlx::postgres::PgRow) -> LivestockLog {
    use sqlx::Row;
    let kind_str: String = row.get("event_kind");
    let cost_val: Option<f64> = row.get("cost");

    LivestockLog {
        id: row.get("id"),
        flock_id: row.get("flock_id"),
        date: row.get("date"),
        event_kind: kind_str.parse().unwrap_or(LivestockEventKind::Other),
        quantity: row.get("quantity"),
        unit: row.get("unit"),
        cost: cost_val.map(Usd::new),
        notes: row.get("notes"),
        created_at: row.get("created_at"),
    }
}
