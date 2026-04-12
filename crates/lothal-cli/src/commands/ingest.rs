use std::env;

use anyhow::{Context, Result};
use indicatif::{ProgressBar, ProgressStyle};
use sqlx::PgPool;
use tracing::info;
use uuid::Uuid;

use lothal_core::ontology::reading::ReadingSource;
use lothal_ingest::mqtt::{MqttConfig, SensorMapping};
use lothal_ingest::nws::NwsConfig;

// ---------------------------------------------------------------------------
// MQTT ingest
// ---------------------------------------------------------------------------

/// Start an MQTT subscriber that streams readings into the database.
///
/// Configuration is loaded from environment variables:
///   - `MQTT_BROKER`  -- broker URL (e.g. `mqtt://192.168.1.100:1883`)
///   - `MQTT_TOPICS`  -- comma-separated list of topic filters
///   - `MQTT_USER`    -- optional username
///   - `MQTT_PASS`    -- optional password
pub async fn run_mqtt_ingest(pool: &PgPool) -> Result<()> {
    let broker = env::var("MQTT_BROKER")
        .context("MQTT_BROKER env var is required (e.g. mqtt://192.168.1.100:1883)")?;
    let topics_raw = env::var("MQTT_TOPICS")
        .context("MQTT_TOPICS env var is required (comma-separated topic filters)")?;
    let topics: Vec<String> = topics_raw
        .split(',')
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .collect();

    let username = env::var("MQTT_USER").ok();
    let password = env::var("MQTT_PASS").ok();

    let config = MqttConfig {
        broker_url: broker.clone(),
        client_id: format!("lothal-ingest-{}", Uuid::new_v4().as_simple()),
        topics: topics.clone(),
        username,
        password,
    };

    // For a first pass, use a minimal default mapping. In practice these would
    // be loaded from a config file or the database.
    let mappings = default_sensor_mappings();

    println!("Connecting to MQTT broker: {broker}");
    println!("Subscribed topics: {}", topics.join(", "));
    println!("Sensor mappings loaded: {}", mappings.len());
    println!();

    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::with_template("{spinner:.green} [{elapsed_precise}] {msg}")
            .expect("valid template"),
    );
    spinner.set_message("Waiting for MQTT messages...");
    spinner.enable_steady_tick(std::time::Duration::from_millis(120));

    let (tx, mut rx) = tokio::sync::mpsc::channel(256);

    // Spawn the subscriber in a background task.
    let subscriber_handle = tokio::spawn(async move {
        if let Err(e) = lothal_ingest::mqtt::run_mqtt_subscriber(config, mappings, tx).await {
            tracing::error!(error = %e, "MQTT subscriber exited with error");
        }
    });

    let mut count: u64 = 0;
    let pool = pool.clone();

    // Consume readings and persist them to the database.
    while let Some(reading) = rx.recv().await {
        lothal_db::reading::insert_reading(&pool, &reading).await?;
        count += 1;
        spinner.set_message(format!(
            "Ingested {count} readings (latest: {} = {:.2} {})",
            reading.source.source_type(),
            reading.value,
            reading.kind,
        ));
    }

    spinner.finish_with_message(format!("MQTT subscriber stopped after {count} readings"));
    subscriber_handle.abort();
    Ok(())
}

/// Build a simple default set of sensor mappings.
///
/// In a real deployment these would come from the database or a YAML config.
/// This provides a reasonable starting point for Home Assistant + Emporia Vue.
fn default_sensor_mappings() -> Vec<SensorMapping> {
    use lothal_core::ontology::reading::ReadingKind;

    vec![
        SensorMapping {
            entity_pattern: "sensor.emporia_vue_total_power".into(),
            source: ReadingSource::Meter(Uuid::nil()),
            kind: ReadingKind::ElectricWatts,
        },
        SensorMapping {
            entity_pattern: "sensor.indoor_temperature".into(),
            source: ReadingSource::Device(Uuid::nil()),
            kind: ReadingKind::TemperatureF,
        },
        SensorMapping {
            entity_pattern: "sensor.indoor_humidity".into(),
            source: ReadingSource::Device(Uuid::nil()),
            kind: ReadingKind::HumidityPct,
        },
    ]
}

// ---------------------------------------------------------------------------
// NWS weather fetch
// ---------------------------------------------------------------------------

/// Fetch weather observations from the National Weather Service for the past
/// `days` days and insert them into the database.
///
/// Requires:
///   - `NWS_STATION` env var (e.g. `KGOK`)
///   - At least one site in the database (uses the first site found)
pub async fn fetch_weather(pool: &PgPool, days: u32) -> Result<()> {
    let station = env::var("NWS_STATION")
        .unwrap_or_else(|_| "KGOK".into());

    // Resolve the site so we can tag observations with the correct site_id.
    let sites = lothal_db::site::list_sites(pool).await?;
    let site = sites
        .first()
        .context("No sites found in the database. Run `lothal init` first.")?;

    let config = NwsConfig {
        station_id: station.clone(),
        user_agent: "lothal-cli/0.1 (github.com/lothal)".into(),
    };

    let now = chrono::Utc::now();
    let start = now - chrono::Duration::days(i64::from(days));

    println!("Fetching weather from NWS station {station}");
    println!("Period: {} to {}", start.format("%Y-%m-%d"), now.format("%Y-%m-%d"));
    println!();

    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::with_template("{spinner:.cyan} {msg}")
            .expect("valid template"),
    );
    pb.set_message("Fetching observations from NWS API...");
    pb.enable_steady_tick(std::time::Duration::from_millis(120));

    let mut observations =
        lothal_ingest::nws::fetch_observations_range(&config, start, now).await?;

    pb.set_message(format!("Received {} observations, inserting into DB...", observations.len()));

    // Stamp each observation with the correct site_id.
    for obs in &mut observations {
        obs.site_id = site.id;
    }

    let total = observations.len();
    lothal_db::weather::insert_weather_batch(pool, &observations).await?;

    pb.finish_and_clear();

    println!("Weather fetch complete:");
    println!("  Station:      {station}");
    println!("  Site:         {} ({})", site.address, site.city);
    println!("  Observations: {total}");
    if let (Some(first), Some(last)) = (observations.first(), observations.last()) {
        println!(
            "  Time range:   {} to {}",
            first.time.format("%Y-%m-%d %H:%M"),
            last.time.format("%Y-%m-%d %H:%M"),
        );
    }

    info!(station = %station, count = total, "weather fetch complete");
    Ok(())
}
