use std::sync::Arc;

use lothal_core::ReadingEvent;
use sqlx::PgPool;
use tokio::sync::broadcast;

/// Shared application state for all Axum handlers.
#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    /// Broadcast channel for pushing live [`ReadingEvent`]s to WebSocket
    /// clients. Published to by the in-process MQTT ingester (if enabled).
    pub readings_tx: broadcast::Sender<ReadingEvent>,
    /// Registry of ontology actions invocable from the web UI.
    pub registry: Arc<lothal_ontology::ActionRegistry>,
    /// Registry of LLM functions. `None` when no provider is configured;
    /// the chat handler renders a canonical error bubble in that case.
    pub llm_functions: Option<Arc<lothal_ontology::LlmFunctionRegistry>>,
}
