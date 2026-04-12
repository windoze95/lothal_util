use sqlx::PgPool;
use tokio::sync::broadcast;

/// Shared application state for all Axum handlers.
#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    /// Broadcast channel for pushing JSON-serialized readings to WebSocket clients.
    pub readings_tx: broadcast::Sender<String>,
}
