use chrono::{DateTime, NaiveDate, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use lothal_core::ontology::weather::WeatherObservation;

/// A daily weather summary row returned from SQL aggregation.
#[derive(Debug, Clone)]
pub struct DailyWeatherRow {
    pub date: NaiveDate,
    pub site_id: Uuid,
    pub avg_temp_f: f64,
    pub min_temp_f: f64,
    pub max_temp_f: f64,
    pub avg_humidity_pct: Option<f64>,
    pub observation_count: i64,
}

// ---------------------------------------------------------------------------
// WeatherObservation
// ---------------------------------------------------------------------------

pub async fn insert_weather_observation(
    pool: &PgPool,
    obs: &WeatherObservation,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"INSERT INTO weather_observations (time, site_id, temperature_f, humidity_pct,
                                              wind_speed_mph, wind_direction_deg,
                                              solar_irradiance_wm2, pressure_inhg, conditions)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"#,
    )
    .bind(obs.time)
    .bind(obs.site_id)
    .bind(obs.temperature_f)
    .bind(obs.humidity_pct)
    .bind(obs.wind_speed_mph)
    .bind(obs.wind_direction_deg)
    .bind(obs.solar_irradiance_wm2)
    .bind(obs.pressure_inhg)
    .bind(&obs.conditions)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn insert_weather_batch(
    pool: &PgPool,
    observations: &[WeatherObservation],
) -> Result<(), sqlx::Error> {
    if observations.is_empty() {
        return Ok(());
    }

    let mut times = Vec::with_capacity(observations.len());
    let mut site_ids = Vec::with_capacity(observations.len());
    let mut temps = Vec::with_capacity(observations.len());
    let mut humidities = Vec::with_capacity(observations.len());
    let mut wind_speeds = Vec::with_capacity(observations.len());
    let mut wind_dirs = Vec::with_capacity(observations.len());
    let mut solar_vals = Vec::with_capacity(observations.len());
    let mut pressures = Vec::with_capacity(observations.len());
    let mut conditions_vec: Vec<Option<String>> = Vec::with_capacity(observations.len());

    for obs in observations {
        times.push(obs.time);
        site_ids.push(obs.site_id);
        temps.push(obs.temperature_f);
        humidities.push(obs.humidity_pct);
        wind_speeds.push(obs.wind_speed_mph);
        wind_dirs.push(obs.wind_direction_deg);
        solar_vals.push(obs.solar_irradiance_wm2);
        pressures.push(obs.pressure_inhg);
        conditions_vec.push(obs.conditions.clone());
    }

    sqlx::query(
        r#"INSERT INTO weather_observations
               (time, site_id, temperature_f, humidity_pct,
                wind_speed_mph, wind_direction_deg, solar_irradiance_wm2,
                pressure_inhg, conditions)
           SELECT * FROM UNNEST(
               $1::timestamptz[], $2::uuid[], $3::float8[], $4::float8[],
               $5::float8[], $6::float8[], $7::float8[], $8::float8[], $9::text[]
           )"#,
    )
    .bind(&times)
    .bind(&site_ids)
    .bind(&temps)
    .bind(&humidities)
    .bind(&wind_speeds)
    .bind(&wind_dirs)
    .bind(&solar_vals)
    .bind(&pressures)
    .bind(&conditions_vec)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn get_weather_range(
    pool: &PgPool,
    site_id: Uuid,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Result<Vec<WeatherObservation>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT time, site_id, temperature_f, humidity_pct,
                wind_speed_mph, wind_direction_deg, solar_irradiance_wm2,
                pressure_inhg, conditions
         FROM weather_observations
         WHERE site_id = $1 AND time >= $2 AND time < $3
         ORDER BY time",
    )
    .bind(site_id)
    .bind(start)
    .bind(end)
    .fetch_all(pool)
    .await?;

    Ok(rows.iter().map(observation_from_row).collect())
}

pub async fn get_daily_weather_summaries(
    pool: &PgPool,
    site_id: Uuid,
    start: NaiveDate,
    end: NaiveDate,
) -> Result<Vec<DailyWeatherRow>, sqlx::Error> {
    let rows = sqlx::query(
        r#"SELECT
               time::date AS date,
               site_id,
               AVG(temperature_f)  AS avg_temp_f,
               MIN(temperature_f)  AS min_temp_f,
               MAX(temperature_f)  AS max_temp_f,
               AVG(humidity_pct)   AS avg_humidity_pct,
               COUNT(*)            AS observation_count
           FROM weather_observations
           WHERE site_id = $1
             AND time::date >= $2
             AND time::date < $3
             AND temperature_f IS NOT NULL
           GROUP BY time::date, site_id
           ORDER BY date"#,
    )
    .bind(site_id)
    .bind(start)
    .bind(end)
    .fetch_all(pool)
    .await?;

    Ok(rows.iter().map(daily_row_from_row).collect())
}

fn observation_from_row(row: &sqlx::postgres::PgRow) -> WeatherObservation {
    use sqlx::Row;
    WeatherObservation {
        time: row.get("time"),
        site_id: row.get("site_id"),
        temperature_f: row.get("temperature_f"),
        humidity_pct: row.get("humidity_pct"),
        wind_speed_mph: row.get("wind_speed_mph"),
        wind_direction_deg: row.get("wind_direction_deg"),
        solar_irradiance_wm2: row.get("solar_irradiance_wm2"),
        pressure_inhg: row.get("pressure_inhg"),
        conditions: row.get("conditions"),
        // New microclimate fields — may not exist in rows from old queries.
        source: row.try_get("source").ok(),
        station_id: row.try_get("station_id").ok().flatten(),
        rainfall_inches: row.try_get("rainfall_inches").ok().flatten(),
    }
}

fn daily_row_from_row(row: &sqlx::postgres::PgRow) -> DailyWeatherRow {
    use sqlx::Row;
    DailyWeatherRow {
        date: row.get("date"),
        site_id: row.get("site_id"),
        avg_temp_f: row.get("avg_temp_f"),
        min_temp_f: row.get("min_temp_f"),
        max_temp_f: row.get("max_temp_f"),
        avg_humidity_pct: row.get("avg_humidity_pct"),
        observation_count: row.get("observation_count"),
    }
}
