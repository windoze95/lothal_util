use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use lothal_core::ontology::reading::{Reading, ReadingKind, ReadingSource};

// ---------------------------------------------------------------------------
// Reading
// ---------------------------------------------------------------------------

pub async fn insert_reading(pool: &PgPool, reading: &Reading) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"INSERT INTO readings (time, source_type, source_id, kind, value, metadata)
           VALUES ($1, $2, $3, $4, $5, $6)"#,
    )
    .bind(reading.time)
    .bind(reading.source.source_type())
    .bind(reading.source.source_id())
    .bind(reading.kind.as_str())
    .bind(reading.value)
    .bind(&reading.metadata)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn insert_readings_batch(
    pool: &PgPool,
    readings: &[Reading],
) -> Result<(), sqlx::Error> {
    if readings.is_empty() {
        return Ok(());
    }

    // Build a bulk insert using UNNEST for efficiency.
    let mut times = Vec::with_capacity(readings.len());
    let mut source_types = Vec::with_capacity(readings.len());
    let mut source_ids = Vec::with_capacity(readings.len());
    let mut kinds = Vec::with_capacity(readings.len());
    let mut values = Vec::with_capacity(readings.len());
    let mut metadatas: Vec<Option<serde_json::Value>> = Vec::with_capacity(readings.len());

    for r in readings {
        times.push(r.time);
        source_types.push(r.source.source_type().to_string());
        source_ids.push(r.source.source_id());
        kinds.push(r.kind.as_str().to_string());
        values.push(r.value);
        metadatas.push(r.metadata.clone());
    }

    sqlx::query(
        r#"INSERT INTO readings (time, source_type, source_id, kind, value, metadata)
           SELECT * FROM UNNEST($1::timestamptz[], $2::text[], $3::uuid[], $4::text[], $5::float8[], $6::jsonb[])"#,
    )
    .bind(&times)
    .bind(&source_types)
    .bind(&source_ids)
    .bind(&kinds)
    .bind(&values)
    .bind(&metadatas)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn get_readings(
    pool: &PgPool,
    source_type: &str,
    source_id: Uuid,
    kind: &str,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Result<Vec<Reading>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT time, source_type, source_id, kind, value, metadata
         FROM readings
         WHERE source_type = $1 AND source_id = $2 AND kind = $3
               AND time >= $4 AND time < $5
         ORDER BY time",
    )
    .bind(source_type)
    .bind(source_id)
    .bind(kind)
    .bind(start)
    .bind(end)
    .fetch_all(pool)
    .await?;

    Ok(rows.iter().map(reading_from_row).collect())
}

pub async fn get_latest_reading(
    pool: &PgPool,
    source_type: &str,
    source_id: Uuid,
    kind: &str,
) -> Result<Option<Reading>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT time, source_type, source_id, kind, value, metadata
         FROM readings
         WHERE source_type = $1 AND source_id = $2 AND kind = $3
         ORDER BY time DESC LIMIT 1",
    )
    .bind(source_type)
    .bind(source_id)
    .bind(kind)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| reading_from_row(&r)))
}

fn reading_from_row(row: &sqlx::postgres::PgRow) -> Reading {
    use sqlx::Row;
    let source_type: String = row.get("source_type");
    let source_id: Uuid = row.get("source_id");
    let kind_str: String = row.get("kind");

    let source = match source_type.as_str() {
        "device" => ReadingSource::Device(source_id),
        "circuit" => ReadingSource::Circuit(source_id),
        "zone" => ReadingSource::Zone(source_id),
        "meter" => ReadingSource::Meter(source_id),
        "property_zone" => ReadingSource::PropertyZone(source_id),
        "pool" => ReadingSource::Pool(source_id),
        "weather_station" => ReadingSource::WeatherStation(source_id),
        _ => ReadingSource::Device(source_id), // fallback
    };

    let kind = kind_str
        .parse::<ReadingKind>()
        .unwrap_or(ReadingKind::ElectricKwh);

    Reading {
        time: row.get("time"),
        source,
        kind,
        value: row.get("value"),
        metadata: row.get("metadata"),
    }
}
