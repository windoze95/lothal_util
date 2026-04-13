pub mod tools;

use serde_json::{json, Value};
use sqlx::PgPool;
use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader};

use lothal_ontology::ActionRegistry;

use crate::AiError;

/// Build the action registry this server exposes. Once `with_defaults` lands
/// in `lothal-ontology`, swap this to `ActionRegistry::with_defaults(pool)`;
/// for now the registry starts empty and actions surface as per-action tools
/// as soon as callers register them.
fn build_action_registry(_pool: &PgPool) -> ActionRegistry {
    ActionRegistry::new()
}

/// Run the MCP server, reading JSON-RPC requests from stdin and writing
/// responses to stdout. This implements the Model Context Protocol for use
/// with Claude Desktop and similar MCP hosts.
pub async fn run_server(pool: PgPool) -> Result<(), AiError> {
    let registry = build_action_registry(&pool);
    run_server_with_registry(pool, registry).await
}

/// Variant of `run_server` that accepts a pre-built `ActionRegistry`. Tests
/// and embedders can pre-populate the registry with custom actions.
pub async fn run_server_with_registry(
    pool: PgPool,
    registry: ActionRegistry,
) -> Result<(), AiError> {
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    let reader = BufReader::new(stdin);
    let mut lines = reader.lines();

    tracing::info!("MCP server started, listening on stdin");

    while let Ok(Some(line)) = lines.next_line().await {
        let line = line.trim().to_string();
        if line.is_empty() {
            continue;
        }

        let request: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                let err_resp = json_rpc_error(Value::Null, -32700, &format!("Parse error: {e}"));
                write_response(&mut stdout, &err_resp).await?;
                continue;
            }
        };

        let response = handle_request(&request, &pool, &registry).await;
        write_response(&mut stdout, &response).await?;
    }

    Ok(())
}

async fn handle_request(request: &Value, pool: &PgPool, registry: &ActionRegistry) -> Value {
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    let method = request["method"].as_str().unwrap_or("");

    match method {
        "initialize" => json_rpc_result(
            id,
            json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {
                    "tools": {}
                },
                "serverInfo": {
                    "name": "lothal",
                    "version": env!("CARGO_PKG_VERSION"),
                }
            }),
        ),

        "notifications/initialized" => {
            // Client acknowledgment, no response needed for notifications.
            Value::Null
        }

        "tools/list" => {
            json_rpc_result(id, json!({ "tools": tools::tool_definitions(registry) }))
        }

        "tools/call" => {
            let tool_name = request["params"]["name"].as_str().unwrap_or("");
            let arguments = request["params"]["arguments"].clone();

            match tools::call_tool(tool_name, arguments, pool, registry).await {
                Ok(result) => json_rpc_result(
                    id,
                    json!({
                        "content": [{
                            "type": "text",
                            "text": serde_json::to_string_pretty(&result).unwrap_or_default()
                        }]
                    }),
                ),
                Err(e) => json_rpc_result(
                    id,
                    json!({
                        "content": [{
                            "type": "text",
                            "text": format!("Error: {e}")
                        }],
                        "isError": true
                    }),
                ),
            }
        }

        _ => json_rpc_error(id, -32601, &format!("Method not found: {method}")),
    }
}

fn json_rpc_result(id: Value, result: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result,
    })
}

fn json_rpc_error(id: Value, code: i32, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": message,
        },
    })
}

async fn write_response(
    stdout: &mut io::Stdout,
    response: &Value,
) -> Result<(), AiError> {
    if response.is_null() {
        return Ok(()); // Notifications don't get responses.
    }
    let mut out = serde_json::to_string(response)
        .map_err(|e| AiError::Mcp(format!("Serialize error: {e}")))?;
    out.push('\n');
    stdout
        .write_all(out.as_bytes())
        .await
        .map_err(|e| AiError::Mcp(format!("Write error: {e}")))?;
    stdout
        .flush()
        .await
        .map_err(|e| AiError::Mcp(format!("Flush error: {e}")))?;
    Ok(())
}
