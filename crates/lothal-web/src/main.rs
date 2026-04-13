mod charts;
mod error;
mod routes;
mod state;
mod templates;

use std::net::SocketAddr;

use tower_http::compression::CompressionLayer;
use tracing_subscriber::EnvFilter;
use uuid::Uuid;

use lothal_core::{ReadingEvent, ReadingKind, ReadingSource};
use lothal_ingest::mqtt::{MqttConfig, SensorMapping};
use state::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load .env and initialize tracing.
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    // Connect to database.
    let database_url =
        std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let pool = lothal_db::create_pool(&database_url).await?;
    lothal_db::run_migrations(&pool).await?;

    tracing::info!("database connected and migrations applied");

    let (readings_tx, _) = tokio::sync::broadcast::channel::<ReadingEvent>(256);
    let registry = std::sync::Arc::new(lothal_ontology::ActionRegistry::with_defaults(pool.clone()));

    // If MQTT_BROKER is set (and LOTHAL_MQTT_IN_WEB != "false"), run an
    // in-process subscriber so the web server can forward live readings over
    // WebSocket without relying on an external daemon.
    //
    // Architecture choice (A) from the design doc: one-node simplicity. For
    // multi-node deployments, switch to Postgres LISTEN/NOTIFY (option B).
    maybe_spawn_mqtt(pool.clone(), readings_tx.clone());

    let state = AppState {
        pool,
        readings_tx,
        registry,
    };

    let app = routes::build_router()
        .layer(CompressionLayer::new())
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    tracing::info!(%addr, "lothal-web starting");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

/// Spawn an in-process MQTT subscriber if `MQTT_BROKER` is set and the opt-out
/// flag `LOTHAL_MQTT_IN_WEB=false` isn't present.
///
/// Writes each reading to the DB (as the daemon would) and additionally
/// broadcasts a [`ReadingEvent`] on `readings_tx` for WebSocket subscribers.
fn maybe_spawn_mqtt(
    pool: sqlx::PgPool,
    readings_tx: tokio::sync::broadcast::Sender<ReadingEvent>,
) {
    let in_web = std::env::var("LOTHAL_MQTT_IN_WEB")
        .map(|v| !v.eq_ignore_ascii_case("false"))
        .unwrap_or(true);
    if !in_web {
        tracing::info!("LOTHAL_MQTT_IN_WEB=false — skipping in-process MQTT subscriber");
        return;
    }
    let broker = match std::env::var("MQTT_BROKER") {
        Ok(b) if !b.is_empty() => b,
        _ => {
            tracing::info!("MQTT_BROKER not set — WebSocket feed will be inactive");
            return;
        }
    };
    let topics_raw = std::env::var("MQTT_TOPICS").unwrap_or_default();
    let topics: Vec<String> = topics_raw
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if topics.is_empty() {
        tracing::warn!("MQTT_BROKER set but MQTT_TOPICS is empty — skipping MQTT subscriber");
        return;
    }

    let config = MqttConfig {
        broker_url: broker.clone(),
        client_id: format!("lothal-web-{}", Uuid::new_v4().as_simple()),
        topics,
        username: std::env::var("MQTT_USER").ok(),
        password: std::env::var("MQTT_PASS").ok(),
    };
    let mappings = default_sensor_mappings();

    tracing::info!(broker = %broker, "starting in-process MQTT subscriber for WebSocket feed");
    tokio::spawn(async move {
        if let Err(e) = lothal_ingest::mqtt::run_subscriber(
            pool,
            Some(readings_tx),
            config,
            mappings,
        )
        .await
        {
            tracing::error!(error = %e, "MQTT subscriber exited");
        }
    });
}

/// Minimal default sensor mappings matching `lothal-cli`'s defaults. In a
/// production deployment these would come from the database or a YAML file.
fn default_sensor_mappings() -> Vec<SensorMapping> {
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
