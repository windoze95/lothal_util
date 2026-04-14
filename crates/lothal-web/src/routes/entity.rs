//! Universal entity view (W1).
//!
//! One page layout for every ontology object, keyed by `(kind, id)`. The
//! layout is always the same — Properties / Timeline / Graph / Actions —
//! regardless of whether the object is a `site`, `device`, `flock`, or a
//! thing invented next week.

use axum::extract::{Form, Path, State};
use axum::http::{header, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use lothal_ai::mcp::tools;
use lothal_ontology::query::{self, ViewOptions};
use lothal_ontology::{ObjectRef, ObjectUri};

use crate::error::WebError;
use crate::state::AppState;
use crate::templates::{EntityPage, EntityTimelinePartial, PropertyRow, TimelineEvent};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/e/{kind}/{id}", get(entity_page))
        .route("/e/{kind}/{id}/timeline", get(timeline_partial))
        .route("/e/{kind}/{id}/graph", get(graph_partial))
        .route("/e/{kind}/{id}/actions/{name}", post(run_action))
        .route("/e/{kind}/{id}/chat", post(chat_send))
}

// ---------------------------------------------------------------------------
// Full-page entity view
// ---------------------------------------------------------------------------

async fn entity_page(
    State(state): State<AppState>,
    Path((kind, id_str)): Path<(String, String)>,
) -> Result<EntityPage, WebError> {
    let id = Uuid::parse_str(&id_str)
        .map_err(|e| WebError::BadRequest(format!("invalid uuid: {e}")))?;
    let uri = ObjectUri::new(kind.clone(), id);

    let view = query::get_object_view(
        &state.pool,
        &uri,
        ViewOptions {
            event_limit: 50,
            neighbor_depth: 1,
        },
    )
    .await
    .map_err(|e| match e {
        sqlx::Error::RowNotFound => WebError::NotFound,
        other => WebError::Database(other),
    })?;

    // Flatten the object's JSONB properties into ordered rows. Askama cannot
    // iterate a `serde_json::Value` directly, so we render each value into a
    // human-friendly string here.
    let properties = properties_to_rows(&view.object.properties.0);

    // Actions are discovered from the registry at request time — future
    // registrations appear without a web-crate code change.
    let applicable_actions = state.registry.applicable_for(&kind);

    let site_name = first_site_address(&state.pool).await.unwrap_or_default();

    Ok(EntityPage {
        active_page: "entity".into(),
        site_name,
        kind,
        id: id.to_string(),
        display_name: view.object.display_name.clone(),
        properties,
        applicable_actions,
    })
}

// ---------------------------------------------------------------------------
// Timeline htmx partial
// ---------------------------------------------------------------------------

async fn timeline_partial(
    State(state): State<AppState>,
    Path((kind, id_str)): Path<(String, String)>,
) -> Result<EntityTimelinePartial, WebError> {
    let id = Uuid::parse_str(&id_str)
        .map_err(|e| WebError::BadRequest(format!("invalid uuid: {e}")))?;
    let uri = ObjectUri::new(kind, id);

    let view = query::get_object_view(
        &state.pool,
        &uri,
        ViewOptions {
            event_limit: 50,
            neighbor_depth: 0,
        },
    )
    .await
    .map_err(|e| match e {
        sqlx::Error::RowNotFound => WebError::NotFound,
        other => WebError::Database(other),
    })?;

    let events = view
        .recent_events
        .into_iter()
        .map(|ev| TimelineEvent {
            time: ev.time.format("%Y-%m-%d %H:%M UTC").to_string(),
            kind: ev.kind,
            summary: ev.summary,
            severity: ev.severity,
        })
        .collect();

    Ok(EntityTimelinePartial { events })
}

// ---------------------------------------------------------------------------
// Neighbor graph JSON
// ---------------------------------------------------------------------------

async fn graph_partial(
    State(state): State<AppState>,
    Path((kind, id_str)): Path<(String, String)>,
) -> Result<Response, WebError> {
    let id = Uuid::parse_str(&id_str)
        .map_err(|e| WebError::BadRequest(format!("invalid uuid: {e}")))?;
    let uri = ObjectUri::new(kind.clone(), id);

    let neighbors = query::neighbors(&state.pool, &uri, None)
        .await
        .map_err(WebError::Database)?;

    // Resolve the root object once so the central node is always present,
    // even when it has no neighbors yet.
    let root = query::get_object_view(
        &state.pool,
        &uri,
        ViewOptions {
            event_limit: 0,
            neighbor_depth: 0,
        },
    )
    .await
    .ok();

    // Build the d3-force payload: every node is identified by "kind:id" so the
    // frontend can match `links.source` / `links.target` by string without
    // needing to pre-index.
    let mut nodes: Vec<Value> = Vec::with_capacity(neighbors.len() + 1);
    let root_node_id = format!("{}:{}", kind, id);
    nodes.push(serde_json::json!({
        "id": root_node_id,
        "kind": kind,
        "name": root.as_ref().map(|v| v.object.display_name.clone()).unwrap_or_else(|| id.to_string()),
        "root": true,
    }));
    let mut links: Vec<Value> = Vec::with_capacity(neighbors.len());
    for (link, other) in &neighbors {
        let other_id = format!("{}:{}", other.kind, other.id);
        nodes.push(serde_json::json!({
            "id": other_id,
            "kind": other.kind,
            "name": other.display_name,
        }));
        // Preserve original direction: src -> dst as recorded on the link.
        let src = format!("{}:{}", link.src_kind, link.src_id);
        let dst = format!("{}:{}", link.dst_kind, link.dst_id);
        links.push(serde_json::json!({
            "source": src,
            "target": dst,
            "kind": link.kind,
        }));
    }

    let body = serde_json::json!({ "nodes": nodes, "links": links });
    Ok((
        [(header::CONTENT_TYPE, "application/json")],
        serde_json::to_string(&body).unwrap_or_else(|_| "{}".into()),
    )
        .into_response())
}

// ---------------------------------------------------------------------------
// Action invocation
// ---------------------------------------------------------------------------

async fn run_action(
    State(state): State<AppState>,
    Path((kind, id_str, name)): Path<(String, String, String)>,
    Json(input): Json<Value>,
) -> Response {
    let id = match Uuid::parse_str(&id_str) {
        Ok(v) => v,
        Err(e) => {
            return html_fragment(
                StatusCode::BAD_REQUEST,
                &format!(
                    r#"<div class="text-sm text-[#f76c6c]">Invalid id: {e}</div>"#
                ),
            );
        }
    };
    let subjects = vec![ObjectRef::new(kind.clone(), id)];

    match state
        .registry
        .invoke(&name, "web:user", state.pool.clone(), subjects, input)
        .await
    {
        Ok(run) => {
            let output_pretty = run
                .output
                .as_ref()
                .map(|o| {
                    serde_json::to_string_pretty(&o.0)
                        .unwrap_or_else(|_| "<unserializable>".into())
                })
                .unwrap_or_else(|| "(no output)".into());
            let body = format!(
                r#"<div class="text-sm">
  <p class="text-[#3dd68c] font-medium mb-2">Action <code>{name}</code> completed ({status}).</p>
  <pre class="bg-[#0f1117] rounded-md p-3 text-xs text-[#e8eaed] overflow-x-auto border border-[#2e3346]">{output}</pre>
</div>"#,
                name = html_escape(&name),
                status = html_escape(&run.status),
                output = html_escape(&output_pretty),
            );
            html_fragment(StatusCode::OK, &body)
        }
        Err(err) => {
            let body = format!(
                r#"<div class="text-sm text-[#f76c6c]">Action <code>{name}</code> failed: {err}</div>"#,
                name = html_escape(&name),
                err = html_escape(&err.to_string()),
            );
            html_fragment(StatusCode::OK, &body)
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn html_fragment(status: StatusCode, body: &str) -> Response {
    (status, Html(body.to_string())).into_response()
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// Render a JSONB properties object as a flat list of `PropertyRow`s.
///
/// Nested objects and arrays are pretty-printed with `to_string_pretty`; scalar
/// values are rendered directly so strings don't carry their JSON quotes.
fn properties_to_rows(value: &Value) -> Vec<PropertyRow> {
    match value {
        Value::Object(map) => map
            .iter()
            .map(|(k, v)| match v {
                Value::Object(_) | Value::Array(_) => PropertyRow {
                    key: k.clone(),
                    value: serde_json::to_string_pretty(v)
                        .unwrap_or_else(|_| v.to_string()),
                    nested: true,
                },
                Value::String(s) => PropertyRow {
                    key: k.clone(),
                    value: s.clone(),
                    nested: false,
                },
                Value::Null => PropertyRow {
                    key: k.clone(),
                    value: "—".into(),
                    nested: false,
                },
                other => PropertyRow {
                    key: k.clone(),
                    value: other.to_string(),
                    nested: false,
                },
            })
            .collect(),
        // Unusual shape — render the whole blob as a single nested row.
        other => vec![PropertyRow {
            key: "value".into(),
            value: serde_json::to_string_pretty(other).unwrap_or_else(|_| other.to_string()),
            nested: true,
        }],
    }
}

async fn first_site_address(pool: &sqlx::PgPool) -> Option<String> {
    lothal_db::site::list_sites(pool)
        .await
        .ok()
        .and_then(|v| v.into_iter().next())
        .map(|s| s.address)
}

// ---------------------------------------------------------------------------
// Chat (entity-scoped, tool-enabled)
// ---------------------------------------------------------------------------

/// Maximum number of tool-use rounds before giving up and returning whatever
/// assistant text was last produced. Each round writes one `llm_calls` trace
/// row via the `entity_chat` function; tool dispatch happens between rounds
/// and doesn't itself hit the LLM.
const CHAT_MAX_TOOL_ROUNDS: usize = 5;

#[derive(Debug, Deserialize)]
pub struct ChatForm {
    pub message: String,
}

async fn chat_send(
    State(state): State<AppState>,
    Path((kind, id_str)): Path<(String, String)>,
    Form(form): Form<ChatForm>,
) -> Result<Html<String>, WebError> {
    let id = Uuid::parse_str(&id_str)
        .map_err(|e| WebError::BadRequest(format!("invalid uuid: {e}")))?;
    let uri = ObjectUri::new(kind.clone(), id);

    // Fetch a lightweight view to get display_name for the entity preamble.
    // Failures bubble up as 404/500.
    let view = query::get_object_view(
        &state.pool,
        &uri,
        ViewOptions {
            event_limit: 0,
            neighbor_depth: 0,
        },
    )
    .await
    .map_err(|e| match e {
        sqlx::Error::RowNotFound => WebError::NotFound,
        other => WebError::Database(other),
    })?;
    let display_name = view.object.display_name.clone();

    // Always render the user's message as a right-aligned bubble first.
    let user_bubble = render_user_bubble(&form.message);

    let functions = match &state.llm_functions {
        Some(f) => f.clone(),
        None => {
            return Ok(Html(format!(
                "{user_bubble}{bubble}",
                bubble = render_error_bubble(
                    "LLM not configured. Set ANTHROPIC_API_KEY (or LOTHAL_FRONTIER_PROVIDER).",
                ),
            )));
        }
    };

    // Entity context goes in the first user message so the `entity_chat`
    // function's system prompt (and thus its sha256 prompt_hash) stays
    // constant across entities.
    let display_or_kind = if display_name.is_empty() {
        kind.as_str()
    } else {
        display_name.as_str()
    };
    let preamble = format!(
        "I'm viewing entity lothal://{kind}/{id} — a {display_or_kind} ({kind}).\n\n{msg}",
        msg = form.message,
    );

    let tool_catalog = build_tool_catalog(&state.registry);
    let mut messages: Vec<Value> = vec![json!({
        "role": "user",
        "content": [{ "type": "text", "text": preamble }],
    })];

    let mut last_text = String::new();

    // Tool-use loop: each iteration invokes `entity_chat` (which writes one
    // `llm_calls` row), parses the returned content blocks, dispatches any
    // tool_use blocks locally, and feeds tool_result blocks back for the
    // next round. Capped at CHAT_MAX_TOOL_ROUNDS.
    for _ in 0..CHAT_MAX_TOOL_ROUNDS {
        let call = match functions
            .invoke(
                "entity_chat",
                "web:chat",
                state.pool.clone(),
                json!({ "messages": messages, "tools": tool_catalog }),
                None,
                None,
            )
            .await
        {
            Ok(c) => c,
            Err(e) => {
                return Ok(Html(format!(
                    "{user_bubble}{bubble}",
                    bubble = render_error_bubble(&format!("entity_chat failed: {e}")),
                )));
            }
        };

        let content: Vec<Value> = call
            .output
            .as_ref()
            .and_then(|v| v.0.get("content"))
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        let round_text = content
            .iter()
            .filter(|b| b["type"] == "text")
            .filter_map(|b| b["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n");
        if !round_text.is_empty() {
            last_text = round_text;
        }

        let tool_uses: Vec<&Value> =
            content.iter().filter(|b| b["type"] == "tool_use").collect();
        if tool_uses.is_empty() {
            break;
        }

        // Echo the assistant turn verbatim so tool_use_ids line up.
        messages.push(json!({ "role": "assistant", "content": content }));

        // Dispatch each tool call and append a single user turn whose
        // content is the list of tool_result blocks (echoing tool_use_id).
        let mut tool_results: Vec<Value> = Vec::with_capacity(tool_uses.len());
        for block in tool_uses {
            let tool_use_id = block["id"].as_str().unwrap_or("").to_string();
            let tool_name = block["name"].as_str().unwrap_or("");
            let args = block["input"].clone();
            let (text, is_error) =
                match tools::call_tool(tool_name, args, &state.pool, &state.registry).await {
                    Ok(v) => (
                        serde_json::to_string(&v).unwrap_or_else(|_| "{}".into()),
                        false,
                    ),
                    Err(e) => (format!("Error: {e}"), true),
                };
            tool_results.push(json!({
                "type": "tool_result",
                "tool_use_id": tool_use_id,
                "content": text,
                "is_error": is_error,
            }));
        }
        messages.push(json!({ "role": "user", "content": tool_results }));
    }

    let assistant_bubble = if last_text.is_empty() {
        render_error_bubble(
            "No response from the assistant (tool-use limit reached without final text).",
        )
    } else {
        render_assistant_bubble(&last_text)
    };
    Ok(Html(format!("{user_bubble}{assistant_bubble}")))
}

/// Translate MCP tool definitions (camelCase `inputSchema`) into Anthropic's
/// Messages-API shape (`input_schema`).
fn build_tool_catalog(registry: &lothal_ontology::ActionRegistry) -> Vec<Value> {
    tools::tool_definitions(registry)
        .into_iter()
        .map(|mut t| {
            if let Some(obj) = t.as_object_mut() {
                if let Some(schema) = obj.remove("inputSchema") {
                    obj.insert("input_schema".into(), schema);
                }
            }
            t
        })
        .collect()
}

fn render_user_bubble(message: &str) -> String {
    format!(
        r#"<div class="flex justify-end"><div class="max-w-[85%] bg-[#4f9cf7] text-white rounded-2xl rounded-br-sm px-4 py-2 text-sm whitespace-pre-wrap break-words">{text}</div></div>"#,
        text = html_escape(message),
    )
}

fn render_assistant_bubble(message: &str) -> String {
    format!(
        r#"<div class="flex justify-start"><div class="max-w-[85%] bg-[#232736] text-[#e8eaed] border border-[#2e3346] rounded-2xl rounded-bl-sm px-4 py-2 text-sm whitespace-pre-wrap break-words">{text}</div></div>"#,
        text = html_escape(message),
    )
}

fn render_error_bubble(message: &str) -> String {
    format!(
        r#"<div class="flex justify-start"><div class="max-w-[85%] bg-[#2a1d22] text-[#f76c6c] border border-[#f76c6c]/40 rounded-2xl rounded-bl-sm px-4 py-2 text-sm whitespace-pre-wrap break-words">{text}</div></div>"#,
        text = html_escape(message),
    )
}
