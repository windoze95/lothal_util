mod ollama;
mod anthropic;

use lothal_ontology::llm_function::ModelTier;

use crate::AiError;

pub use ollama::OllamaProvider;
pub use anthropic::AnthropicProvider;

/// A message in a conversation.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

#[derive(Debug, Clone, Copy, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
}

/// Request to an LLM provider.
#[derive(Debug, Clone)]
pub struct CompletionRequest {
    pub system: String,
    pub messages: Vec<Message>,
    pub max_tokens: u32,
    pub temperature: f32,
    /// When set, enables extended thinking with this many budget tokens.
    /// Anthropic only — ignored for Ollama. Temperature is forced to 1
    /// when thinking is enabled (API requirement).
    pub budget_tokens: Option<u32>,
}

/// Response from an LLM provider.
#[derive(Debug, Clone)]
pub struct CompletionResponse {
    pub content: String,
    pub model: String,
    pub input_tokens: Option<u32>,
    pub output_tokens: Option<u32>,
}

/// A single provider backend. Owned by [`LlmClient`] as up to two slots
/// (local + frontier) so calls can be routed per [`ModelTier`].
enum Backend {
    Ollama(OllamaProvider),
    Anthropic(AnthropicProvider),
}

impl Backend {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse, AiError> {
        match self {
            Self::Ollama(p) => p.complete(request).await,
            Self::Anthropic(p) => p.complete(request).await,
        }
    }

    async fn complete_json(
        &self,
        request: &CompletionRequest,
        schema: &serde_json::Value,
    ) -> Result<serde_json::Value, AiError> {
        match self {
            Self::Ollama(p) => p.complete_json(request, schema).await,
            Self::Anthropic(p) => p.complete_json(request, schema).await,
        }
    }

    async fn check_status(&self) -> Result<String, AiError> {
        match self {
            Self::Ollama(p) => p.check_status().await,
            Self::Anthropic(p) => p.check_status().await,
        }
    }

    fn provider_name(&self) -> &'static str {
        match self {
            Self::Ollama(_) => "ollama",
            Self::Anthropic(_) => "anthropic",
        }
    }

    fn model_name(&self) -> &str {
        match self {
            Self::Ollama(p) => &p.model,
            Self::Anthropic(p) => &p.model,
        }
    }
}

/// Unified LLM client that holds up to two provider backends — one for
/// [`ModelTier::Local`] and one for [`ModelTier::Frontier`] — and routes
/// calls per-tier.
///
/// Both slots are optional. If only one is configured, calls to the missing
/// tier fall back to the configured one so no feature silently stops working
/// when (e.g.) Ollama isn't running locally.
pub struct LlmClient {
    local: Option<Backend>,
    frontier: Option<Backend>,
}

impl LlmClient {
    /// Build the client from environment variables.
    ///
    /// Each tier reads its own provider variable, falling back to the
    /// per-tier default and (for the frontier tier only) the legacy
    /// `LOTHAL_LLM_PROVIDER` fallback:
    ///
    /// - **Local tier** → `LOTHAL_LOCAL_PROVIDER` (default: `ollama`).
    /// - **Frontier tier** → `LOTHAL_FRONTIER_PROVIDER` (default: `anthropic`).
    ///   If unset, honours legacy `LOTHAL_LLM_PROVIDER` before falling back
    ///   to the default.
    ///
    /// Provider-specific vars (`OLLAMA_BASE_URL`, `OLLAMA_MODEL`,
    /// `ANTHROPIC_API_KEY`, `ANTHROPIC_MODEL`) are shared by whichever tier
    /// selects that provider.
    ///
    /// Returns an error only when *no* provider could be constructed for
    /// *either* tier.
    pub fn from_env() -> Result<Self, AiError> {
        let local = build_tier_provider(ModelTier::Local).ok();
        let frontier = build_tier_provider(ModelTier::Frontier).ok();

        if local.is_none() && frontier.is_none() {
            return Err(AiError::ProviderNotConfigured(
                "no LLM provider could be constructed; set ANTHROPIC_API_KEY or OLLAMA_BASE_URL".into(),
            ));
        }
        Ok(Self { local, frontier })
    }

    /// Resolve a tier to a backend, falling back to the other tier if the
    /// primary is not configured.
    fn pick(&self, tier: ModelTier) -> Result<&Backend, AiError> {
        let (primary, fallback) = match tier {
            ModelTier::Local => (&self.local, &self.frontier),
            ModelTier::Frontier => (&self.frontier, &self.local),
        };
        primary
            .as_ref()
            .or(fallback.as_ref())
            .ok_or_else(|| {
                AiError::ProviderNotConfigured(format!(
                    "no provider available for tier {:?}",
                    tier
                ))
            })
    }

    /// Complete for a specific tier. Used by [`crate::invoker::LlmClientInvoker`].
    pub async fn complete_for_tier(
        &self,
        tier: ModelTier,
        request: &CompletionRequest,
    ) -> Result<CompletionResponse, AiError> {
        self.pick(tier)?.complete(request).await
    }

    /// JSON-mode completion for a specific tier.
    pub async fn complete_json_for_tier(
        &self,
        tier: ModelTier,
        request: &CompletionRequest,
        schema: &serde_json::Value,
    ) -> Result<serde_json::Value, AiError> {
        self.pick(tier)?.complete_json(request, schema).await
    }

    /// The model name resolved for a tier. Useful when `complete_json`
    /// doesn't carry the model through its return value.
    pub fn model_name_for_tier(&self, tier: ModelTier) -> Option<&str> {
        self.pick(tier).ok().map(|b| b.model_name())
    }

    /// Legacy / tier-agnostic completion. Routes to the frontier tier;
    /// existing callers (e.g. bill extraction validators) keep working
    /// without change.
    pub async fn complete(
        &self,
        request: &CompletionRequest,
    ) -> Result<CompletionResponse, AiError> {
        self.complete_for_tier(ModelTier::Frontier, request).await
    }

    /// Legacy / tier-agnostic JSON completion. Routes to the frontier tier.
    pub async fn complete_json(
        &self,
        request: &CompletionRequest,
        schema: &serde_json::Value,
    ) -> Result<serde_json::Value, AiError> {
        self.complete_json_for_tier(ModelTier::Frontier, request, schema)
            .await
    }

    /// Check connectivity across all configured tiers.
    pub async fn check_status(&self) -> Result<String, AiError> {
        let mut parts = Vec::new();
        if let Some(b) = &self.local {
            parts.push(format!("local({}): {}", b.provider_name(), b.check_status().await?));
        }
        if let Some(b) = &self.frontier {
            parts.push(format!("frontier({}): {}", b.provider_name(), b.check_status().await?));
        }
        if parts.is_empty() {
            return Err(AiError::ProviderNotConfigured("nothing to check".into()));
        }
        Ok(parts.join(", "))
    }

    /// Legacy accessor — returns the frontier provider name, or local if
    /// frontier isn't configured. Status-line display only.
    pub fn provider_name(&self) -> &str {
        self.frontier
            .as_ref()
            .or(self.local.as_ref())
            .map(|b| b.provider_name())
            .unwrap_or("none")
    }

    /// Legacy accessor — returns the frontier model name, or local's if
    /// frontier isn't configured.
    pub fn model_name(&self) -> &str {
        self.frontier
            .as_ref()
            .or(self.local.as_ref())
            .map(|b| b.model_name())
            .unwrap_or("none")
    }
}

/// Build a [`Backend`] for the given tier from env. Returns an error only
/// when the selected provider's required vars are missing.
fn build_tier_provider(tier: ModelTier) -> Result<Backend, AiError> {
    let provider_name = resolve_tier_provider_name(tier);
    match provider_name.to_lowercase().as_str() {
        "ollama" => {
            let base_url =
                std::env::var("OLLAMA_BASE_URL").unwrap_or_else(|_| "http://localhost:11434".into());
            let model = std::env::var("OLLAMA_MODEL").unwrap_or_else(|_| "gemma4:31b".into());
            Ok(Backend::Ollama(OllamaProvider::new(base_url, model)))
        }
        "anthropic" => {
            let api_key = std::env::var("ANTHROPIC_API_KEY").map_err(|_| {
                AiError::ProviderNotConfigured(
                    "ANTHROPIC_API_KEY must be set when using anthropic provider".into(),
                )
            })?;
            let model =
                std::env::var("ANTHROPIC_MODEL").unwrap_or_else(|_| "claude-opus-4-6".into());
            Ok(Backend::Anthropic(AnthropicProvider::new(api_key, model)))
        }
        other => Err(AiError::ProviderNotConfigured(format!(
            "Unknown provider '{other}'. Use 'ollama' or 'anthropic'."
        ))),
    }
}

fn resolve_tier_provider_name(tier: ModelTier) -> String {
    match tier {
        ModelTier::Local => std::env::var("LOTHAL_LOCAL_PROVIDER").unwrap_or_else(|_| "ollama".into()),
        ModelTier::Frontier => std::env::var("LOTHAL_FRONTIER_PROVIDER")
            .or_else(|_| std::env::var("LOTHAL_LLM_PROVIDER"))
            .unwrap_or_else(|_| "anthropic".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Unit-test the env-resolution rules that drive tier routing.
    ///
    /// Each test sets its own tier-specific vars before calling
    /// `resolve_tier_provider_name` to avoid coupling to global env state.
    /// Serialized via a mutex because `std::env::set_var` is process-global.
    use std::sync::Mutex;
    static ENV_MUTEX: Mutex<()> = Mutex::new(());

    fn with_env<T>(set: &[(&str, Option<&str>)], f: impl FnOnce() -> T) -> T {
        let _guard = ENV_MUTEX.lock().unwrap();
        // SAFETY: guarded by ENV_MUTEX; no other test mutates env concurrently.
        unsafe {
            let saved: Vec<_> = set
                .iter()
                .map(|(k, _)| (*k, std::env::var(k).ok()))
                .collect();
            for (k, v) in set {
                match v {
                    Some(val) => std::env::set_var(k, val),
                    None => std::env::remove_var(k),
                }
            }
            let out = f();
            for (k, v) in saved {
                match v {
                    Some(val) => std::env::set_var(k, val),
                    None => std::env::remove_var(k),
                }
            }
            out
        }
    }

    #[test]
    fn local_tier_honours_its_own_var() {
        let name = with_env(
            &[
                ("LOTHAL_LOCAL_PROVIDER", Some("anthropic")),
                ("LOTHAL_FRONTIER_PROVIDER", None),
                ("LOTHAL_LLM_PROVIDER", None),
            ],
            || resolve_tier_provider_name(ModelTier::Local),
        );
        assert_eq!(name, "anthropic");
    }

    #[test]
    fn local_tier_defaults_to_ollama() {
        let name = with_env(
            &[("LOTHAL_LOCAL_PROVIDER", None)],
            || resolve_tier_provider_name(ModelTier::Local),
        );
        assert_eq!(name, "ollama");
    }

    #[test]
    fn frontier_tier_falls_back_to_legacy_var() {
        let name = with_env(
            &[
                ("LOTHAL_FRONTIER_PROVIDER", None),
                ("LOTHAL_LLM_PROVIDER", Some("ollama")),
            ],
            || resolve_tier_provider_name(ModelTier::Frontier),
        );
        assert_eq!(name, "ollama");
    }

    #[test]
    fn frontier_tier_prefers_tier_var_over_legacy() {
        let name = with_env(
            &[
                ("LOTHAL_FRONTIER_PROVIDER", Some("anthropic")),
                ("LOTHAL_LLM_PROVIDER", Some("ollama")),
            ],
            || resolve_tier_provider_name(ModelTier::Frontier),
        );
        assert_eq!(name, "anthropic");
    }

    #[test]
    fn frontier_tier_defaults_to_anthropic() {
        let name = with_env(
            &[
                ("LOTHAL_FRONTIER_PROVIDER", None),
                ("LOTHAL_LLM_PROVIDER", None),
            ],
            || resolve_tier_provider_name(ModelTier::Frontier),
        );
        assert_eq!(name, "anthropic");
    }
}
