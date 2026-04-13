//! Persistence helpers for `llm_calls` — the audit trail of every LLM
//! invocation routed through the `LlmFunctionRegistry`.
//!
//! All helpers use the `llm_calls` table defined in migration
//! `002_llm_calls.sql`. The shape mirrors `action_runs` (pending → running →
//! succeeded/failed) with extra columns for tier, model, prompt_hash, tokens,
//! latency, and the nullable links to a parent action run or conversation
//! thread.

use uuid::Uuid;

use super::ModelTier;

/// Persisted record of a single LLM invocation.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct LlmCall {
    pub id: Uuid,
    pub function_name: String,
    pub status: String,
    pub invoked_by: String,
    pub tier: String,
    pub prompt_hash: String,
    pub model: Option<String>,
    pub input: sqlx::types::Json<serde_json::Value>,
    pub output: Option<sqlx::types::Json<serde_json::Value>>,
    pub error: Option<String>,
    pub tokens_in: Option<i32>,
    pub tokens_out: Option<i32>,
    pub latency_ms: Option<i64>,
    pub parent_action_run_id: Option<Uuid>,
    pub thread_id: Option<Uuid>,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub finished_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Insert a new `llm_calls` row in `pending` status and return its id.
pub(crate) async fn insert_pending(
    pool: &sqlx::PgPool,
    function_name: &str,
    invoked_by: &str,
    tier: ModelTier,
    prompt_hash: &str,
    input: &serde_json::Value,
    parent_action_run_id: Option<Uuid>,
    thread_id: Option<Uuid>,
) -> Result<Uuid, sqlx::Error> {
    let row: (Uuid,) = sqlx::query_as(
        r#"
        INSERT INTO llm_calls (
            function_name, status, invoked_by, tier, prompt_hash, input,
            parent_action_run_id, thread_id
        )
        VALUES ($1, 'pending', $2, $3, $4, $5, $6, $7)
        RETURNING id
        "#,
    )
    .bind(function_name)
    .bind(invoked_by)
    .bind(tier.as_str())
    .bind(prompt_hash)
    .bind(sqlx::types::Json(input))
    .bind(parent_action_run_id)
    .bind(thread_id)
    .fetch_one(pool)
    .await?;
    Ok(row.0)
}

/// Flip an LLM call from `pending` to `running`.
pub(crate) async fn mark_running(pool: &sqlx::PgPool, id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE llm_calls SET status = 'running' WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Finalize an LLM call as `succeeded` with its output + usage metadata.
pub(crate) async fn mark_succeeded(
    pool: &sqlx::PgPool,
    id: Uuid,
    output: &serde_json::Value,
    model: &str,
    tokens_in: Option<u32>,
    tokens_out: Option<u32>,
    latency_ms: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        UPDATE llm_calls
        SET status = 'succeeded',
            output = $2,
            model = $3,
            tokens_in = $4,
            tokens_out = $5,
            latency_ms = $6,
            finished_at = now()
        WHERE id = $1
        "#,
    )
    .bind(id)
    .bind(sqlx::types::Json(output))
    .bind(model)
    .bind(tokens_in.map(|v| v as i32))
    .bind(tokens_out.map(|v| v as i32))
    .bind(latency_ms)
    .execute(pool)
    .await?;
    Ok(())
}

/// Finalize an LLM call as `failed` with its error message.
pub(crate) async fn mark_failed(
    pool: &sqlx::PgPool,
    id: Uuid,
    error: &str,
    latency_ms: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        UPDATE llm_calls
        SET status = 'failed',
            error = $2,
            latency_ms = $3,
            finished_at = now()
        WHERE id = $1
        "#,
    )
    .bind(id)
    .bind(error)
    .bind(latency_ms)
    .execute(pool)
    .await?;
    Ok(())
}

/// Fetch a full `LlmCall` row by id.
pub(crate) async fn load_by_id(pool: &sqlx::PgPool, id: Uuid) -> Result<LlmCall, sqlx::Error> {
    sqlx::query_as::<_, LlmCall>(
        r#"
        SELECT id, function_name, status, invoked_by, tier, prompt_hash, model,
               input, output, error, tokens_in, tokens_out, latency_ms,
               parent_action_run_id, thread_id, started_at, finished_at
        FROM llm_calls
        WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_one(pool)
    .await
}
