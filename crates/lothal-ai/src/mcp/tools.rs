//! Generic MCP tool surface backed by the ontology query layer + action registry.
//!
//! Six fixed "primitive" tools expose the read-side ontology API
//! (`get_object`, `neighbors`, `events`, `timeline`, `search`, `run_action`).
//! Additionally, every action registered in the provided `ActionRegistry`
//! surfaces as a typed tool named after the action, with its declared
//! `input_schema`, so agents can invoke actions by name without routing
//! through the generic `run_action` dispatcher.

use chrono::{DateTime, NaiveDate, Utc};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

use lothal_ontology::action::run::ActionRun;
use lothal_ontology::query::{self as oq, ViewOptions};
use lothal_ontology::{
    ActionRegistry, EventRecord, LinkRecord, ObjectRecord, ObjectRef, ObjectUri,
};

use crate::AiError;

// ---------------------------------------------------------------------------
// Tool definitions
// ---------------------------------------------------------------------------

/// Return the MCP tool definitions for tools/list: the six generic primitives
/// plus one tool per registered action.
pub fn tool_definitions(registry: &ActionRegistry) -> Vec<Value> {
    let mut tools = generic_tool_definitions();
    for action in registry.list() {
        tools.push(json!({
            "name": action.name(),
            "description": action.description(),
            "inputSchema": action_input_schema(action.applicable_kinds(), action.input_schema()),
        }));
    }
    tools
}

fn generic_tool_definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "get_object",
            "description": "Fetch an ontology object by URI together with its \
                depth-1 neighbors and recent events. URI format: `lothal://{kind}/{uuid}`.",
            "inputSchema": {
                "type": "object",
                "required": ["uri"],
                "properties": {
                    "uri": { "type": "string", "description": "`lothal://{kind}/{uuid}`" }
                }
            }
        }),
        json!({
            "name": "neighbors",
            "description": "List neighbors of an ontology object (bidirectional, active \
                links only). Optional `link_kind` filter.",
            "inputSchema": {
                "type": "object",
                "required": ["uri"],
                "properties": {
                    "uri":       { "type": "string", "description": "`lothal://{kind}/{uuid}`" },
                    "link_kind": { "type": "string", "description": "Restrict to links of this kind" }
                }
            }
        }),
        json!({
            "name": "events",
            "description": "Query events whose `subjects` array contains any of the supplied \
                URIs within [from_time, to_time). Optional `kind` filter.",
            "inputSchema": {
                "type": "object",
                "required": ["uris", "from_time", "to_time"],
                "properties": {
                    "uris":      { "type": "array", "items": { "type": "string" },
                                   "description": "List of `lothal://{kind}/{uuid}` URIs" },
                    "from_time": { "type": "string", "description": "ISO-8601 datetime (inclusive)" },
                    "to_time":   { "type": "string", "description": "ISO-8601 datetime (exclusive)" },
                    "kind":      { "type": "string", "description": "Event kind filter" }
                }
            }
        }),
        json!({
            "name": "timeline",
            "description": "Return events mentioning a single URI between two instants. \
                Equivalent to `events` with a one-element URI list.",
            "inputSchema": {
                "type": "object",
                "required": ["uri", "from_time", "to_time"],
                "properties": {
                    "uri":       { "type": "string", "description": "`lothal://{kind}/{uuid}`" },
                    "from_time": { "type": "string", "description": "ISO-8601 datetime (inclusive)" },
                    "to_time":   { "type": "string", "description": "ISO-8601 datetime (exclusive)" }
                }
            }
        }),
        json!({
            "name": "search",
            "description": "Full-text search across ontology objects. Optional kind filter; \
                `limit` defaults to 25 and is clamped to [1, 200].",
            "inputSchema": {
                "type": "object",
                "required": ["query"],
                "properties": {
                    "query": { "type": "string" },
                    "kind":  { "type": "string" },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 200 }
                }
            }
        }),
        json!({
            "name": "run_action",
            "description": "Generic dispatcher: invoke a registered action by name against \
                a set of subjects. Prefer the typed per-action tools when available.",
            "inputSchema": {
                "type": "object",
                "required": ["name", "subjects", "input"],
                "properties": {
                    "name":     { "type": "string" },
                    "subjects": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "required": ["kind", "id"],
                            "properties": {
                                "kind": { "type": "string" },
                                "id":   { "type": "string", "description": "UUID" }
                            }
                        }
                    },
                    "input":    { "type": "object" }
                }
            }
        }),
    ]
}

/// Wrap an action's declared `input_schema` so the tool's arguments always
/// include the subjects list alongside the action-defined input payload.
fn action_input_schema(applicable_kinds: &[&'static str], input_schema: Value) -> Value {
    json!({
        "type": "object",
        "required": ["subjects", "input"],
        "properties": {
            "subjects": {
                "type": "array",
                "description": format!(
                    "Subjects this action applies to (kinds: {}).",
                    applicable_kinds.join(", ")
                ),
                "items": {
                    "type": "object",
                    "required": ["kind", "id"],
                    "properties": {
                        "kind": { "type": "string", "enum": applicable_kinds },
                        "id":   { "type": "string", "description": "UUID" }
                    }
                }
            },
            "input": input_schema
        }
    })
}

// ---------------------------------------------------------------------------
// Dispatch
// ---------------------------------------------------------------------------

/// Route a tool call to the appropriate handler. Unknown names that match a
/// registered action are invoked through the registry with the tool's
/// `{subjects, input}` payload; anything else is a protocol error.
pub async fn call_tool(
    name: &str,
    args: Value,
    pool: &PgPool,
    registry: &ActionRegistry,
) -> Result<Value, AiError> {
    match name {
        "get_object" => handle_get_object(args, pool).await,
        "neighbors" => handle_neighbors(args, pool).await,
        "events" => handle_events(args, pool).await,
        "timeline" => handle_timeline(args, pool).await,
        "search" => handle_search(args, pool).await,
        "run_action" => handle_run_action(args, pool, registry).await,
        other => {
            if registry.get(other).is_some() {
                handle_action_tool(other, args, pool, registry).await
            } else {
                Err(AiError::Mcp(format!("Unknown tool: {other}")))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Generic tool handlers
// ---------------------------------------------------------------------------

async fn handle_get_object(args: Value, pool: &PgPool) -> Result<Value, AiError> {
    let uri = parse_required_uri(&args, "uri")?;
    let view = oq::get_object_view(pool, &uri, ViewOptions::default())
        .await
        .map_err(AiError::Database)?;

    let neighbors: Vec<Value> = view
        .neighbors
        .into_iter()
        .map(|(link, obj)| {
            json!({
                "link": link_to_json(&link),
                "object": object_to_json(&obj),
            })
        })
        .collect();
    let recent_events: Vec<Value> = view.recent_events.iter().map(event_to_json).collect();

    Ok(json!({
        "object": object_to_json(&view.object),
        "neighbors": neighbors,
        "recent_events": recent_events,
        "applicable_actions": view.applicable_actions,
    }))
}

async fn handle_neighbors(args: Value, pool: &PgPool) -> Result<Value, AiError> {
    let uri = parse_required_uri(&args, "uri")?;
    let link_kind = args.get("link_kind").and_then(|v| v.as_str());

    let rows = oq::neighbors(pool, &uri, link_kind)
        .await
        .map_err(AiError::Database)?;
    let out: Vec<Value> = rows
        .into_iter()
        .map(|(link, obj)| {
            json!({
                "link": link_to_json(&link),
                "object": object_to_json(&obj),
            })
        })
        .collect();
    Ok(Value::Array(out))
}

async fn handle_events(args: Value, pool: &PgPool) -> Result<Value, AiError> {
    let uris = parse_uri_array(&args, "uris")?;
    let from = parse_required_datetime(&args, "from_time")?;
    let to = parse_required_datetime(&args, "to_time")?;
    let kind = args.get("kind").and_then(|v| v.as_str());

    let events = oq::events_for(pool, &uris, from, to, kind)
        .await
        .map_err(AiError::Database)?;
    Ok(Value::Array(events.iter().map(event_to_json).collect()))
}

async fn handle_timeline(args: Value, pool: &PgPool) -> Result<Value, AiError> {
    let uri = parse_required_uri(&args, "uri")?;
    let from = parse_required_datetime(&args, "from_time")?;
    let to = parse_required_datetime(&args, "to_time")?;

    let events = oq::events_for(pool, std::slice::from_ref(&uri), from, to, None)
        .await
        .map_err(AiError::Database)?;
    Ok(Value::Array(events.iter().map(event_to_json).collect()))
}

async fn handle_search(args: Value, pool: &PgPool) -> Result<Value, AiError> {
    let query = args["query"]
        .as_str()
        .ok_or_else(|| AiError::Validation("query is required".into()))?;
    let kind = args.get("kind").and_then(|v| v.as_str());
    let limit = args
        .get("limit")
        .and_then(|v| v.as_u64())
        .map(|n| n as usize)
        .unwrap_or(25);

    let rows = oq::search(pool, query, kind, limit)
        .await
        .map_err(AiError::Database)?;
    Ok(Value::Array(rows.iter().map(object_to_json).collect()))
}

async fn handle_run_action(
    args: Value,
    pool: &PgPool,
    registry: &ActionRegistry,
) -> Result<Value, AiError> {
    let name = args["name"]
        .as_str()
        .ok_or_else(|| AiError::Validation("name is required".into()))?
        .to_string();
    let subjects = parse_subject_array(&args, "subjects")?;
    let input = args.get("input").cloned().unwrap_or(json!({}));

    let run = registry
        .invoke(&name, "agent:mcp", pool.clone(), subjects, input)
        .await
        .map_err(action_err_to_ai)?;
    Ok(action_run_to_json(&run))
}

/// Handle a typed per-action tool call. Expects `{subjects, input}` in args
/// and routes to `ActionRegistry::invoke` with the tool's name as the action.
async fn handle_action_tool(
    name: &str,
    args: Value,
    pool: &PgPool,
    registry: &ActionRegistry,
) -> Result<Value, AiError> {
    let subjects = parse_subject_array(&args, "subjects")?;
    let input = args.get("input").cloned().unwrap_or(json!({}));

    let run = registry
        .invoke(name, "agent:mcp", pool.clone(), subjects, input)
        .await
        .map_err(action_err_to_ai)?;
    Ok(action_run_to_json(&run))
}

// ---------------------------------------------------------------------------
// Parsing helpers
// ---------------------------------------------------------------------------

fn parse_required_uri(args: &Value, field: &str) -> Result<ObjectUri, AiError> {
    let raw = args[field]
        .as_str()
        .ok_or_else(|| AiError::Validation(format!("{field} is required")))?;
    ObjectUri::parse(raw).map_err(|e| AiError::Validation(format!("Invalid URI for {field}: {e}")))
}

fn parse_uri_array(args: &Value, field: &str) -> Result<Vec<ObjectUri>, AiError> {
    let raw = args
        .get(field)
        .and_then(|v| v.as_array())
        .ok_or_else(|| AiError::Validation(format!("{field} must be an array")))?;
    raw.iter()
        .map(|item| {
            let s = item
                .as_str()
                .ok_or_else(|| AiError::Validation(format!("{field} entries must be strings")))?;
            ObjectUri::parse(s)
                .map_err(|e| AiError::Validation(format!("Invalid URI in {field}: {e}")))
        })
        .collect()
}

fn parse_subject_array(args: &Value, field: &str) -> Result<Vec<ObjectRef>, AiError> {
    let raw = args
        .get(field)
        .and_then(|v| v.as_array())
        .ok_or_else(|| AiError::Validation(format!("{field} must be an array")))?;
    raw.iter()
        .map(|item| {
            let obj = item
                .as_object()
                .ok_or_else(|| AiError::Validation(format!("{field} entries must be objects")))?;
            let kind = obj
                .get("kind")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AiError::Validation(format!("{field}[].kind is required")))?;
            let id_str = obj
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AiError::Validation(format!("{field}[].id is required")))?;
            let id = Uuid::parse_str(id_str)
                .map_err(|e| AiError::Validation(format!("{field}[].id must be a UUID: {e}")))?;
            Ok(ObjectRef::new(kind, id))
        })
        .collect()
}

fn parse_required_datetime(args: &Value, field: &str) -> Result<DateTime<Utc>, AiError> {
    let raw = args[field]
        .as_str()
        .ok_or_else(|| AiError::Validation(format!("{field} is required")))?;
    DateTime::parse_from_rfc3339(raw)
        .map(|d| d.with_timezone(&Utc))
        .or_else(|_| {
            NaiveDate::parse_from_str(raw, "%Y-%m-%d")
                .map(|d| d.and_hms_opt(0, 0, 0).unwrap().and_utc())
        })
        .map_err(|e| AiError::Validation(format!("Invalid datetime for {field}: {e}")))
}

// ---------------------------------------------------------------------------
// Serialization helpers
// ---------------------------------------------------------------------------

fn object_to_json(obj: &ObjectRecord) -> Value {
    json!({
        "uri":          format!("lothal://{}/{}", obj.kind, obj.id),
        "kind":         obj.kind,
        "id":           obj.id,
        "display_name": obj.display_name,
        "site_id":      obj.site_id,
        "properties":   obj.properties.0,
        "created_at":   obj.created_at,
        "updated_at":   obj.updated_at,
        "deleted_at":   obj.deleted_at,
    })
}

fn link_to_json(link: &LinkRecord) -> Value {
    json!({
        "id":          link.id,
        "kind":        link.kind,
        "src":         { "kind": link.src_kind, "id": link.src_id },
        "dst":         { "kind": link.dst_kind, "id": link.dst_id },
        "valid_from":  link.valid_from,
        "valid_until": link.valid_until,
        "properties":  link.properties.0,
    })
}

fn event_to_json(event: &EventRecord) -> Value {
    json!({
        "id":         event.id,
        "time":       event.time,
        "kind":       event.kind,
        "site_id":    event.site_id,
        "subjects":   event.subjects.0,
        "summary":    event.summary,
        "severity":   event.severity,
        "properties": event.properties.0,
        "source":     event.source,
    })
}

fn action_run_to_json(run: &ActionRun) -> Value {
    json!({
        "id":           run.id,
        "action_name":  run.action_name,
        "status":       run.status,
        "invoked_by":   run.invoked_by,
        "subjects":     run.subjects.0,
        "input":        run.input.0,
        "output":       run.output.as_ref().map(|j| &j.0),
        "error":        run.error,
        "started_at":   run.started_at,
        "finished_at":  run.finished_at,
    })
}

fn action_err_to_ai(err: lothal_ontology::ActionError) -> AiError {
    use lothal_ontology::ActionError;
    match err {
        ActionError::Unknown(name) => AiError::Validation(format!("Unknown action: {name}")),
        ActionError::InvalidInput(msg) => {
            AiError::Validation(format!("Invalid action input: {msg}"))
        }
        ActionError::NotApplicable(kind) => {
            AiError::Validation(format!("Action not applicable to kind `{kind}`"))
        }
        ActionError::Database(e) => AiError::Database(e),
        ActionError::Other(e) => AiError::Mcp(format!("Action error: {e}")),
    }
}
