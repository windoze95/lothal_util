use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;

pub struct ActionCtx {
    pub pool: sqlx::PgPool,
    pub invoked_by: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ActionError {
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

    pub async fn invoke(
        &self,
        _name: &str,
        _invoked_by: &str,
        _input: serde_json::Value,
    ) -> Result<serde_json::Value, ActionError> {
        todo!()
    }
}

pub mod run;
