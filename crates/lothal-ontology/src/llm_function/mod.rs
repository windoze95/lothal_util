//! `LlmFunction` — declarative, typed LLM-backed function.
//!
//! This is the `AIP Logic`-shaped primitive for lothal. Every LLM call in the
//! system (daily briefings, entity-scoped chat, diagnostic actions, bill
//! extraction, NILM labelling, …) is modelled as an `LlmFunction` so that:
//!
//! - model selection is per-function (via [`ModelTier`]), not a global env var
//! - every invocation gets an `llm_calls` trace row with prompt hash, tokens,
//!   latency, and optional links to a parent action run or chat thread
//! - prompt evolution is safe by construction: when the system prompt changes,
//!   its SHA-256 in the trace changes, and behaviour can be diff'd across
//!   hashes over the event log — no eval-dataset machinery required
//!
//! The registry mirrors [`crate::action::ActionRegistry`]'s pending → running
//! → succeeded/failed shape so the two audit trails compose.
//!
//! Phase 1 records [`ModelTier`] in every trace row but does *not* yet
//! dispatch on it. Phase 3 will add tier-based provider routing to the
//! concrete [`LlmInvoker`] impl in `lothal-ai`; until then the invoker is
//! whatever concrete provider was wired in at registry construction time.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub mod builtin;
pub mod run;

pub use run::LlmCall;

/// Which provider pool to route an LLM call to.
///
/// Phase 1 treats this as metadata (stored on every `llm_calls` row). Phase 3
/// wires [`LlmInvoker`] to honour the tier when picking a provider — at which
/// point `NilmLabelFunction` (Tier::Local) stops paying Anthropic rates while
/// `DailyBriefingFunction` (Tier::Frontier) stops being gated on Ollama being
/// up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelTier {
    /// Local models (Ollama) — fast, cheap, good for narrow classification.
    Local,
    /// Frontier models (Anthropic) — reasoning-heavy, expensive.
    Frontier,
}

impl ModelTier {
    pub fn as_str(&self) -> &'static str {
        match self {
            ModelTier::Local => "local",
            ModelTier::Frontier => "frontier",
        }
    }
}

/// Shape of a single LLM request issued from inside an [`LlmFunction::run`]
/// body. Richer than [`crate::action::LlmCompleter`] (it carries the tier and
/// can return usage/model metadata) so the registry can log costs.
#[derive(Debug, Clone)]
pub struct InvokeRequest {
    pub tier: ModelTier,
    pub system: String,
    pub user: String,
    pub max_tokens: u32,
    pub budget_tokens: Option<u32>,
    /// When `Some`, the provider is asked for JSON conforming to this schema.
    /// When `None`, the provider returns plain text (wrapped as a JSON string
    /// in [`InvokeResponse::content`] so callers can parse uniformly).
    pub json_schema: Option<serde_json::Value>,
}

/// Result of a single LLM call, with enough metadata for the trace row.
#[derive(Debug, Clone)]
pub struct InvokeResponse {
    /// The LLM response as JSON. For plain-text responses this is a JSON
    /// string (`Value::String`); for structured responses it is the parsed
    /// schema-conforming value.
    pub content: serde_json::Value,
    pub model: String,
    pub tokens_in: Option<u32>,
    pub tokens_out: Option<u32>,
}

/// Narrow provider-facing trait carried in `lothal-ontology` so
/// `LlmFunction::run` bodies can issue LLM calls without the ontology crate
/// depending on `lothal-ai` (which would create a cycle). The web/ai layer
/// wires a concrete impl via [`LlmFunctionRegistry::with_invoker`].
#[async_trait]
pub trait LlmInvoker: Send + Sync {
    async fn invoke(&self, req: &InvokeRequest) -> Result<InvokeResponse, anyhow::Error>;
}

/// Execution context handed to every [`LlmFunction::run`] call.
pub struct LlmFunctionCtx {
    pub pool: sqlx::PgPool,
    pub invoked_by: String,
    /// Present when the registry was built with [`LlmFunctionRegistry::with_invoker`].
    /// Functions must error gracefully (not panic) when this is `None`.
    pub invoker: Option<Arc<dyn LlmInvoker>>,
    /// When this function is called from inside an `Action`, the parent
    /// action run's id — stored on the `llm_calls` row so the two audit
    /// trails link.
    pub parent_action_run_id: Option<Uuid>,
    /// Reserved for future conversation-thread persistence. Stays `None` for
    /// stateless entity-scoped chat today.
    pub thread_id: Option<Uuid>,
}

#[derive(Debug, thiserror::Error)]
pub enum LlmFunctionError {
    #[error("unknown llm function: {0}")]
    Unknown(String),
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("llm invoker not configured")]
    NoInvoker,
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

/// Output of a single [`LlmFunction::run`] body. The trait impl is
/// responsible for mapping its internal LLM response into this shape so the
/// registry can record model / tokens / latency on the trace row.
#[derive(Debug, Clone)]
pub struct LlmFunctionOutput {
    /// The function's typed result (conforming to `output_schema`).
    pub output: serde_json::Value,
    /// The raw response metadata for the underlying LLM call. If the
    /// function made multiple calls, this should reflect the *final* call;
    /// intermediate calls can be recorded as child `llm_calls` rows in a
    /// later phase.
    pub response: InvokeResponse,
}

#[async_trait]
pub trait LlmFunction: Send + Sync {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;

    /// Which provider tier should serve this function. Recorded on every
    /// trace row; Phase 3 uses it to route to the concrete provider.
    fn tier(&self) -> ModelTier;

    /// The system prompt string. SHA-256 of this is recorded on every trace
    /// row as `prompt_hash`, giving free content-addressed versioning.
    fn system_prompt(&self) -> &str;

    fn max_tokens(&self) -> u32;
    fn budget_tokens(&self) -> Option<u32> {
        None
    }

    fn input_schema(&self) -> serde_json::Value;
    fn output_schema(&self) -> serde_json::Value;

    /// Run the function body. Implementations build an [`InvokeRequest`],
    /// call `ctx.invoker` (returning [`LlmFunctionError::NoInvoker`] if
    /// missing), and return the typed result along with the raw response
    /// metadata.
    async fn run(
        &self,
        ctx: &LlmFunctionCtx,
        input: serde_json::Value,
    ) -> Result<LlmFunctionOutput, LlmFunctionError>;
}

#[derive(Default)]
pub struct LlmFunctionRegistry {
    functions: HashMap<&'static str, Arc<dyn LlmFunction>>,
    invoker: Option<Arc<dyn LlmInvoker>>,
}

impl LlmFunctionRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Attach a concrete [`LlmInvoker`]. The `lothal-ai` layer owns the
    /// implementation (so this crate stays free of an `lothal-ai` dependency)
    /// and hands it in at registry construction time.
    pub fn with_invoker(mut self, invoker: Arc<dyn LlmInvoker>) -> Self {
        self.invoker = Some(invoker);
        self
    }

    pub fn register(&mut self, function: Arc<dyn LlmFunction>) {
        self.functions.insert(function.name(), function);
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn LlmFunction>> {
        self.functions.get(name).cloned()
    }

    pub fn list(&self) -> Vec<Arc<dyn LlmFunction>> {
        self.functions.values().cloned().collect()
    }

    /// Invoke a registered LLM function, persisting an `llm_calls` audit row
    /// around the call.
    ///
    /// Steps:
    /// 1. Look up the function by name; unknown names return [`LlmFunctionError::Unknown`].
    /// 2. Insert a `pending` row with `sha256(system_prompt)`, flip to `running`.
    /// 3. Run the function body with a fresh [`LlmFunctionCtx`].
    /// 4. On success: record model / tokens / latency, flip to `succeeded`.
    ///    On failure: record latency + error, flip to `failed`.
    /// 5. Return the fully-loaded [`LlmCall`] row.
    pub async fn invoke(
        &self,
        name: &str,
        invoked_by: &str,
        pool: sqlx::PgPool,
        input: serde_json::Value,
        parent_action_run_id: Option<Uuid>,
        thread_id: Option<Uuid>,
    ) -> Result<LlmCall, LlmFunctionError> {
        let function = self
            .get(name)
            .ok_or_else(|| LlmFunctionError::Unknown(name.to_string()))?;

        let prompt_hash = sha256_hex(function.system_prompt());

        let run_id = run::insert_pending(
            &pool,
            name,
            invoked_by,
            function.tier(),
            &prompt_hash,
            &input,
            parent_action_run_id,
            thread_id,
        )
        .await?;
        run::mark_running(&pool, run_id).await?;

        let ctx = LlmFunctionCtx {
            pool: pool.clone(),
            invoked_by: invoked_by.to_string(),
            invoker: self.invoker.clone(),
            parent_action_run_id,
            thread_id,
        };

        let started = Instant::now();
        match function.run(&ctx, input).await {
            Ok(result) => {
                let latency_ms = started.elapsed().as_millis() as i64;
                run::mark_succeeded(
                    &pool,
                    run_id,
                    &result.output,
                    &result.response.model,
                    result.response.tokens_in,
                    result.response.tokens_out,
                    latency_ms,
                )
                .await?;
            }
            Err(err) => {
                let latency_ms = started.elapsed().as_millis() as i64;
                let msg = err.to_string();
                run::mark_failed(&pool, run_id, &msg, latency_ms).await?;
                return Err(err);
            }
        }

        let row = run::load_by_id(&pool, run_id).await?;
        Ok(row)
    }
}

fn sha256_hex(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    let digest = hasher.finalize();
    hex_encode(&digest)
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_is_hex_and_stable() {
        let h1 = sha256_hex("system prompt v1");
        let h2 = sha256_hex("system prompt v1");
        let h3 = sha256_hex("system prompt v2");
        assert_eq!(h1, h2);
        assert_ne!(h1, h3);
        assert_eq!(h1.len(), 64);
        assert!(h1.chars().all(|c| c.is_ascii_hexdigit() && !c.is_uppercase()));
    }

    #[test]
    fn tier_as_str() {
        assert_eq!(ModelTier::Local.as_str(), "local");
        assert_eq!(ModelTier::Frontier.as_str(), "frontier");
    }
}
