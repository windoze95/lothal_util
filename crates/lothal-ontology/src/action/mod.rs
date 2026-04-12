use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;

use crate::{indexer, EventSpec};

pub struct ActionCtx {
    pub pool: sqlx::PgPool,
    pub invoked_by: String,
}

impl ActionCtx {
    /// Emit an event from inside an action body. Opens a short-lived transaction
    /// against `self.pool`, calls `indexer::emit_event`, and commits.
    pub async fn emit_event(&self, ev: EventSpec) -> Result<uuid::Uuid, sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        let id = indexer::emit_event(&mut tx, ev).await?;
        tx.commit().await?;
        Ok(id)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ActionError {
    #[error("unknown action: {0}")]
    Unknown(String),
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("not applicable to kind {0}")]
    NotApplicable(String),
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[async_trait]
pub trait Action: Send + Sync {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn applicable_kinds(&self) -> &'static [&'static str];
    fn input_schema(&self) -> serde_json::Value;
    fn output_schema(&self) -> serde_json::Value;
    async fn run(
        &self,
        ctx: &ActionCtx,
        input: serde_json::Value,
    ) -> Result<serde_json::Value, ActionError>;
}

#[derive(Default)]
pub struct ActionRegistry {
    actions: HashMap<&'static str, Arc<dyn Action>>,
}

impl ActionRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, action: Arc<dyn Action>) {
        self.actions.insert(action.name(), action);
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn Action>> {
        self.actions.get(name).cloned()
    }

    pub fn applicable_for(&self, kind: &str) -> Vec<String> {
        self.actions
            .values()
            .filter(|a| a.applicable_kinds().contains(&kind))
            .map(|a| a.name().to_string())
            .collect()
    }

    pub fn list(&self) -> Vec<Arc<dyn Action>> {
        self.actions.values().cloned().collect()
    }

    /// Invoke a registered action, persisting an `action_runs` audit row and
    /// emitting an `action_completed` / `action_failed` event around the call.
    ///
    /// Steps:
    /// 1. Look up the action by name; unknown names return `ActionError::Unknown`.
    /// 2. Validate every subject kind is in `applicable_kinds()`.
    /// 3. Insert a `pending` row, flip to `running`.
    /// 4. Run the action body with a fresh `ActionCtx`.
    /// 5. On success: mark the row `succeeded` and emit `action_completed`.
    ///    On failure: mark the row `failed` and emit `action_failed` (severity warning).
    /// 6. Return the fully-loaded `ActionRun` row.
    pub async fn invoke(
        &self,
        name: &str,
        invoked_by: &str,
        pool: sqlx::PgPool,
        subjects: Vec<crate::ObjectRef>,
        input: serde_json::Value,
    ) -> Result<crate::action::run::ActionRun, ActionError> {
        // 1. Resolve the action.
        let action = self
            .get(name)
            .ok_or_else(|| ActionError::Unknown(name.to_string()))?;

        // 2. Applicability check.
        let kinds = action.applicable_kinds();
        for s in &subjects {
            if !kinds.iter().any(|k| *k == s.kind.as_str()) {
                return Err(ActionError::NotApplicable(s.kind.clone()));
            }
        }

        // 3. Open the audit row.
        let run_id = run::insert_pending(&pool, name, invoked_by, &subjects, &input).await?;
        run::mark_running(&pool, run_id).await?;

        // 4. Build the context and dispatch.
        let ctx = ActionCtx {
            pool: pool.clone(),
            invoked_by: invoked_by.to_string(),
        };
        // 5. Finalize + emit completion/failure event.
        match action.run(&ctx, input.clone()).await {
            Ok(output) => {
                run::mark_succeeded(&pool, run_id, &output).await?;
                let ev = EventSpec {
                    kind: "action_completed".into(),
                    site_id: None,
                    subjects: subjects.clone(),
                    summary: format!("action {name} completed"),
                    severity: None,
                    properties: serde_json::json!({
                        "action": name,
                        "run_id": run_id,
                        "invoked_by": invoked_by,
                    }),
                    source: format!("action:{name}"),
                };
                ctx.emit_event(ev).await?;
            }
            Err(err) => {
                let msg = err.to_string();
                run::mark_failed(&pool, run_id, &msg).await?;
                let ev = EventSpec {
                    kind: "action_failed".into(),
                    site_id: None,
                    subjects: subjects.clone(),
                    summary: format!("action {name} failed: {msg}"),
                    severity: Some("warning".into()),
                    properties: serde_json::json!({
                        "action": name,
                        "run_id": run_id,
                        "invoked_by": invoked_by,
                        "error": msg,
                    }),
                    source: format!("action:{name}"),
                };
                // Best-effort event emission — surface the original error
                // to the caller even if the audit event write failed.
                let _ = ctx.emit_event(ev).await;
                return Err(err);
            }
        }

        // 6. Hand back the fully-populated row.
        let row = run::load_by_id(&pool, run_id).await?;
        Ok(row)
    }
}

pub mod run;
