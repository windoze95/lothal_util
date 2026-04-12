//! Bridge helpers that expose the `ActionRegistry` as LLM tool definitions.

pub fn ontology_tool_defs(_registry: &crate::ActionRegistry) -> Vec<serde_json::Value> {
    todo!()
}

pub async fn call_tool(
    _registry: &crate::ActionRegistry,
    _pool: &sqlx::PgPool,
    _name: &str,
    _args: serde_json::Value,
) -> Result<serde_json::Value, anyhow::Error> {
    todo!()
}
