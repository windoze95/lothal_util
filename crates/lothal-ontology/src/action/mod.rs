use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;

use crate::{EventSpec, indexer};

pub mod builtin;
pub mod run;

/// Narrow LLM completion trait that the ontology carries without taking a hard
/// dependency on `lothal-ai` (which would create a crate cycle:
/// `lothal-ai -> lothal-db -> lothal-ontology -> lothal-ai`).
///
/// The web/ai layer wires a concrete impl via [`ActionRegistry::with_llm`].
/// Actions that need an LLM should check `ctx.llm` and return
/// `ActionError::Other(anyhow!("Claude client not configured"))` when `None`.
#[async_trait]
pub trait LlmCompleter: Send + Sync {
    /// Complete a prompt and return a plain-text response.
    async fn complete_text(
        &self,
        system: &str,
        user: &str,
        max_tokens: u32,
    ) -> Result<String, anyhow::Error>;

    /// Complete a prompt expecting a JSON response conforming to `schema`.
    async fn complete_json(
        &self,
        system: &str,
        user: &str,
        max_tokens: u32,
        schema: &serde_json::Value,
    ) -> Result<serde_json::Value, anyhow::Error>;
}

pub struct ActionCtx {
    pub pool: sqlx::PgPool,
    pub invoked_by: String,
    /// Optional LLM client. `None` when the registry was built without
    /// `with_llm(..)`. LLM-dependent actions must error gracefully in that
    /// case rather than panic.
    pub llm: Option<Arc<dyn LlmCompleter>>,
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
    /// Run the action body.
    ///
    /// By convention the registry injects the resolved subjects into the
    /// `input` JSON under the `_subjects` key before dispatch, shaped as
    /// `[{"kind": "...", "id": "<uuid>"}, ...]`. Action implementations that
    /// need the subject list should read it from there rather than taking a
    /// separate parameter.
    async fn run(
        &self,
        ctx: &ActionCtx,
        input: serde_json::Value,
    ) -> Result<serde_json::Value, ActionError>;
}

#[derive(Default)]
pub struct ActionRegistry {
    actions: HashMap<&'static str, Arc<dyn Action>>,
    llm: Option<Arc<dyn LlmCompleter>>,
}

impl ActionRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Construct a registry seeded with the default built-in actions.
    ///
    /// Registers: `record_observation`, `schedule_maintenance`,
    /// `run_diagnostic`, `scoped_briefing`, `apply_recommendation`. The stub
    /// action `ingest_bill_pdf` is intentionally left out until it gains a
    /// real body.
    ///
    /// The `pool` is only used by the action bodies via `ActionCtx`; the
    /// parameter is accepted here to keep the constructor signature stable
    /// if future defaults need it at registration time.
    pub fn with_defaults(_pool: sqlx::PgPool) -> Self {
        let mut reg = Self::new();
        reg.register(Arc::new(builtin::record_observation::RecordObservation));
        reg.register(Arc::new(builtin::schedule_maintenance::ScheduleMaintenance));
        reg.register(Arc::new(builtin::run_diagnostic::RunDiagnostic));
        reg.register(Arc::new(builtin::scoped_briefing::ScopedBriefing));
        reg.register(Arc::new(builtin::apply_recommendation::ApplyRecommendation));
        reg
    }

    /// Attach an LLM completer so LLM-dependent actions can run.
    ///
    /// The web/ai layer owns the concrete [`LlmCompleter`] impl (so the
    /// ontology crate stays free of an `lothal-ai` dependency) and hands it
    /// in at registry construction time.
    pub fn with_llm(mut self, llm: Arc<dyn LlmCompleter>) -> Self {
        self.llm = Some(llm);
        self
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
    /// 4. Run the action body with a fresh `ActionCtx`. Subjects are injected
    ///    into the `input` JSON under the `_subjects` key so action bodies can
    ///    read them uniformly.
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

        // 4. Build the context and dispatch. Subjects are threaded through the
        //    input JSON under `_subjects` so each action body can read them
        //    uniformly without a separate parameter.
        let mut input_with_subjects = input.clone();
        if let serde_json::Value::Object(ref mut map) = input_with_subjects {
            let subjects_json = serde_json::Value::Array(
                subjects
                    .iter()
                    .map(|r| serde_json::json!({ "kind": r.kind, "id": r.id }))
                    .collect(),
            );
            map.insert("_subjects".into(), subjects_json);
        }
        let ctx = ActionCtx {
            pool: pool.clone(),
            invoked_by: invoked_by.to_string(),
            llm: self.llm.clone(),
        };
        // 5. Finalize + emit completion/failure event.
        match action.run(&ctx, input_with_subjects).await {
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
