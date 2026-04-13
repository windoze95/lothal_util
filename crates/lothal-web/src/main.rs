mod charts;
mod error;
mod routes;
mod state;
mod templates;

use std::net::SocketAddr;

use tower_http::compression::CompressionLayer;
use tracing_subscriber::EnvFilter;

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

    let (readings_tx, _) = tokio::sync::broadcast::channel(256);
    // ActionRegistry starts empty; once `with_defaults(pool)` is added to
    // `lothal-ontology`, swap this for that. The rest of the web layer
    // already routes every action invocation through `state.registry`, so
    // new defaults light up automatically.
    let registry = std::sync::Arc::new(lothal_ontology::ActionRegistry::new());
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
