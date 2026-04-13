pub mod api;
pub mod entity;
pub mod map;
pub mod pages;
pub mod partials;
pub mod ws;

use axum::Router;
use tower_http::services::ServeDir;

use crate::state::AppState;

/// Build the complete Axum router with all routes.
pub fn build_router() -> Router<AppState> {
    Router::new()
        // Full-page HTML routes
        .merge(pages::router())
        // Universal entity view (W1)
        .merge(entity::router())
        // Property map (W4)
        .merge(map::router())
        // htmx partial fragment routes
        .merge(partials::router())
        // JSON API routes
        .merge(api::router())
        // WebSocket
        .merge(ws::router())
        // Static file serving (CSS, JS, icons)
        .nest_service("/static", ServeDir::new("crates/lothal-web/static"))
}
