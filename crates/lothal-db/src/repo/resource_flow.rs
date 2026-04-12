use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use lothal_core::ontology::resource_flow::{FlowEndpoint, ResourceFlow, ResourceType};
use lothal_core::units::Usd;

// ---------------------------------------------------------------------------
// ResourceFlow
// ---------------------------------------------------------------------------

pub async fn insert_resource_flow(
    pool: &PgPool,
    flow: &ResourceFlow,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"INSERT INTO resource_flows
               (id, site_id, resource_type, source_type, source_id,
                sink_type, sink_id, quantity, unit, cost, timestamp,
                notes, created_at)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)"#,
    )
    .bind(flow.id)
    .bind(flow.site_id)
    .bind(flow.resource_type.to_string())
    .bind(flow.source.endpoint_type())
    .bind(flow.source.sql_id())
    .bind(flow.sink.endpoint_type())
    .bind(flow.sink.sql_id())
    .bind(flow.quantity)
    .bind(&flow.unit)
    .bind(flow.cost.map(|c| c.value()))
    .bind(flow.timestamp)
    .bind(&flow.notes)
    .bind(flow.created_at)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn insert_resource_flows_batch(
    pool: &PgPool,
    flows: &[ResourceFlow],
) -> Result<(), sqlx::Error> {
    if flows.is_empty() {
        return Ok(());
    }

    let mut ids = Vec::with_capacity(flows.len());
    let mut site_ids = Vec::with_capacity(flows.len());
    let mut resource_types = Vec::with_capacity(flows.len());
    let mut source_types = Vec::with_capacity(flows.len());
    let mut source_ids = Vec::with_capacity(flows.len());
    let mut sink_types = Vec::with_capacity(flows.len());
    let mut sink_ids = Vec::with_capacity(flows.len());
    let mut quantities = Vec::with_capacity(flows.len());
    let mut units = Vec::with_capacity(flows.len());
    let mut costs: Vec<Option<f64>> = Vec::with_capacity(flows.len());
    let mut timestamps = Vec::with_capacity(flows.len());
    let mut notes_vec: Vec<Option<String>> = Vec::with_capacity(flows.len());
    let mut created_ats = Vec::with_capacity(flows.len());

    for f in flows {
        ids.push(f.id);
        site_ids.push(f.site_id);
        resource_types.push(f.resource_type.to_string());
        source_types.push(f.source.endpoint_type().to_string());
        source_ids.push(f.source.sql_id());
        sink_types.push(f.sink.endpoint_type().to_string());
        sink_ids.push(f.sink.sql_id());
        quantities.push(f.quantity);
        units.push(f.unit.clone());
        costs.push(f.cost.map(|c| c.value()));
        timestamps.push(f.timestamp);
        notes_vec.push(f.notes.clone());
        created_ats.push(f.created_at);
    }

    sqlx::query(
        r#"INSERT INTO resource_flows
               (id, site_id, resource_type, source_type, source_id,
                sink_type, sink_id, quantity, unit, cost, timestamp,
                notes, created_at)
           SELECT * FROM UNNEST(
               $1::uuid[], $2::uuid[], $3::text[], $4::text[], $5::uuid[],
               $6::text[], $7::uuid[], $8::float8[], $9::text[], $10::float8[],
               $11::timestamptz[], $12::text[], $13::timestamptz[]
           )"#,
    )
    .bind(&ids)
    .bind(&site_ids)
    .bind(&resource_types)
    .bind(&source_types)
    .bind(&source_ids)
    .bind(&sink_types)
    .bind(&sink_ids)
    .bind(&quantities)
    .bind(&units)
    .bind(&costs)
    .bind(&timestamps)
    .bind(&notes_vec)
    .bind(&created_ats)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn list_flows_by_site(
    pool: &PgPool,
    site_id: Uuid,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Result<Vec<ResourceFlow>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, site_id, resource_type, source_type, source_id,
                sink_type, sink_id, quantity, unit, cost, timestamp,
                notes, created_at
         FROM resource_flows
         WHERE site_id = $1 AND timestamp >= $2 AND timestamp < $3
         ORDER BY timestamp",
    )
    .bind(site_id)
    .bind(start)
    .bind(end)
    .fetch_all(pool)
    .await?;

    Ok(rows.iter().map(flow_from_row).collect())
}

pub async fn list_flows_by_type(
    pool: &PgPool,
    site_id: Uuid,
    resource_type: &str,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Result<Vec<ResourceFlow>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, site_id, resource_type, source_type, source_id,
                sink_type, sink_id, quantity, unit, cost, timestamp,
                notes, created_at
         FROM resource_flows
         WHERE site_id = $1 AND resource_type = $2
           AND timestamp >= $3 AND timestamp < $4
         ORDER BY timestamp",
    )
    .bind(site_id)
    .bind(resource_type)
    .bind(start)
    .bind(end)
    .fetch_all(pool)
    .await?;

    Ok(rows.iter().map(flow_from_row).collect())
}

pub async fn list_flows_by_endpoint(
    pool: &PgPool,
    endpoint_type: &str,
    endpoint_id: Uuid,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Result<Vec<ResourceFlow>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, site_id, resource_type, source_type, source_id,
                sink_type, sink_id, quantity, unit, cost, timestamp,
                notes, created_at
         FROM resource_flows
         WHERE ((source_type = $1 AND source_id = $2)
                OR (sink_type = $1 AND sink_id = $2))
           AND timestamp >= $3 AND timestamp < $4
         ORDER BY timestamp",
    )
    .bind(endpoint_type)
    .bind(endpoint_id)
    .bind(start)
    .bind(end)
    .fetch_all(pool)
    .await?;

    Ok(rows.iter().map(flow_from_row).collect())
}

/// Aggregate total in vs out for a resource type in a period.
pub async fn get_flow_balance(
    pool: &PgPool,
    site_id: Uuid,
    resource_type: &str,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Result<FlowBalance, sqlx::Error> {
    let row = sqlx::query_as::<_, (f64, f64)>(
        r#"SELECT
               COALESCE(SUM(CASE WHEN source_type = 'external' THEN quantity ELSE 0 END), 0),
               COALESCE(SUM(CASE WHEN sink_type = 'external' THEN quantity ELSE 0 END), 0)
           FROM resource_flows
           WHERE site_id = $1 AND resource_type = $2
             AND timestamp >= $3 AND timestamp < $4"#,
    )
    .bind(site_id)
    .bind(resource_type)
    .bind(start)
    .bind(end)
    .fetch_one(pool)
    .await?;

    Ok(FlowBalance {
        total_in: row.0,
        total_out: row.1,
        net: row.0 - row.1,
    })
}

#[derive(Debug, Clone)]
pub struct FlowBalance {
    pub total_in: f64,
    pub total_out: f64,
    pub net: f64,
}

fn flow_from_row(row: &sqlx::postgres::PgRow) -> ResourceFlow {
    use sqlx::Row;
    let rt_str: String = row.get("resource_type");
    let src_type: String = row.get("source_type");
    let src_id: Uuid = row.get("source_id");
    let snk_type: String = row.get("sink_type");
    let snk_id: Uuid = row.get("sink_id");
    let cost_val: Option<f64> = row.get("cost");

    ResourceFlow {
        id: row.get("id"),
        site_id: row.get("site_id"),
        resource_type: rt_str.parse().unwrap_or(ResourceType::Water),
        source: FlowEndpoint::from_sql(&src_type, src_id),
        sink: FlowEndpoint::from_sql(&snk_type, snk_id),
        quantity: row.get("quantity"),
        unit: row.get("unit"),
        cost: cost_val.map(Usd::new),
        timestamp: row.get("timestamp"),
        notes: row.get("notes"),
        created_at: row.get("created_at"),
    }
}
