//! WebSocket feed for live sensor readings.
//!
//! Opens at `/ws/readings`. Accepts an optional `uri` query parameter of the
//! form `lothal://{kind}/{id}`; when set, only events matching that
//! source are forwarded. Each event is serialized as a compact JSON text
//! frame: `{"time","uri","kind","value"}`.
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use serde::Deserialize;
use tokio::sync::broadcast::error::RecvError;

use lothal_core::ReadingEvent;

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/ws/readings", get(ws_handler))
}

#[derive(Debug, Deserialize)]
pub struct WsQuery {
    /// Optional filter: only forward events whose source URI matches.
    pub uri: Option<String>,
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    Query(q): Query<WsQuery>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state, q.uri))
}

async fn handle_socket(mut socket: WebSocket, state: AppState, filter_uri: Option<String>) {
    let mut rx = state.readings_tx.subscribe();
    loop {
        tokio::select! {
            // Server -> client: forward broadcast events.
            event = rx.recv() => match event {
                Ok(evt) => {
                    if let Some(ref filter) = filter_uri {
                        if &evt.uri() != filter {
                            continue;
                        }
                    }
                    let payload = match serde_json::to_string(&json_frame(&evt)) {
                        Ok(s) => s,
                        Err(_) => continue,
                    };
                    if socket.send(Message::Text(payload.into())).await.is_err() {
                        break;
                    }
                }
                Err(RecvError::Lagged(skipped)) => {
                    tracing::warn!(skipped, "WS client lagged; dropping events");
                    continue;
                }
                Err(RecvError::Closed) => break,
            },
            // Client -> server: ignore pings/text but watch for disconnect.
            msg = socket.recv() => match msg {
                Some(Ok(Message::Close(_))) | None => break,
                Some(Err(_)) => break,
                _ => {}
            },
        }
    }
}

/// Build the compact JSON frame sent to clients:
/// `{"time", "uri", "kind", "value"}`.
fn json_frame(evt: &ReadingEvent) -> serde_json::Value {
    serde_json::json!({
        "time": evt.time,
        "uri": evt.uri(),
        "kind": evt.kind,
        "value": evt.value,
    })
}
