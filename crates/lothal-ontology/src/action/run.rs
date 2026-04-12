//! Persistence helpers for `action_runs` — the audit trail of every action
//! invocation routed through the `ActionRegistry`.
//!
//! All helpers use the `action_runs` table defined in migration `015_ontology.sql`.

use uuid::Uuid;

use crate::ObjectRef;

/// Persisted record of a single action invocation.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ActionRun {
    pub id: Uuid,
    pub action_name: String,
    pub status: String,
    pub invoked_by: String,
    pub subjects: sqlx::types::Json<serde_json::Value>,
    pub input: sqlx::types::Json<serde_json::Value>,
    pub output: Option<sqlx::types::Json<serde_json::Value>>,
    pub error: Option<String>,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub finished_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Serialize a list of subject refs into the shape stored in `action_runs.subjects`.
fn subjects_to_json(subjects: &[ObjectRef]) -> serde_json::Value {
    serde_json::Value::Array(
        subjects
            .iter()
            .map(|r| serde_json::json!({ "kind": r.kind, "id": r.id }))
            .collect(),
    )
}

/// Insert a new `action_runs` row in `pending` status and return its id.
pub(crate) async fn insert_pending(
    pool: &sqlx::PgPool,
    action_name: &str,
    invoked_by: &str,
    subjects: &[ObjectRef],
    input: &serde_json::Value,
) -> Result<Uuid, sqlx::Error> {
    let row: (Uuid,) = sqlx::query_as(
        r#"
        INSERT INTO action_runs (action_name, status, invoked_by, subjects, input)
        VALUES ($1, 'pending', $2, $3, $4)
        RETURNING id
        "#,
    )
    .bind(action_name)
    .bind(invoked_by)
    .bind(sqlx::types::Json(subjects_to_json(subjects)))
    .bind(sqlx::types::Json(input))
    .fetch_one(pool)
    .await?;
    Ok(row.0)
}

/// Flip an action run from `pending` to `running`.
pub(crate) async fn mark_running(pool: &sqlx::PgPool, id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE action_runs SET status = 'running' WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Finalize an action run as `succeeded` with its output payload.
pub(crate) async fn mark_succeeded(
    pool: &sqlx::PgPool,
    id: Uuid,
    output: &serde_json::Value,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        UPDATE action_runs
        SET status = 'succeeded', output = $2, finished_at = now()
        WHERE id = $1
        "#,
    )
    .bind(id)
    .bind(sqlx::types::Json(output))
    .execute(pool)
    .await?;
    Ok(())
}

/// Finalize an action run as `failed` with its error message.
pub(crate) async fn mark_failed(
    pool: &sqlx::PgPool,
    id: Uuid,
    error: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        UPDATE action_runs
        SET status = 'failed', error = $2, finished_at = now()
        WHERE id = $1
        "#,
    )
    .bind(id)
    .bind(error)
    .execute(pool)
    .await?;
    Ok(())
}

/// Fetch a full `ActionRun` row by id.
pub(crate) async fn load_by_id(pool: &sqlx::PgPool, id: Uuid) -> Result<ActionRun, sqlx::Error> {
    sqlx::query_as::<_, ActionRun>(
        r#"
        SELECT id, action_name, status, invoked_by, subjects, input, output, error,
               started_at, finished_at
        FROM action_runs
        WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_one(pool)
    .await
}
