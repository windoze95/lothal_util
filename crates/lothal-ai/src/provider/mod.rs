mod ollama;
mod anthropic;

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
}

/// Response from an LLM provider.
#[derive(Debug, Clone)]
pub struct CompletionResponse {
    pub content: String,
    pub model: String,
    pub input_tokens: Option<u32>,
    pub output_tokens: Option<u32>,
}

/// Unified LLM client that dispatches to the configured provider.
pub enum LlmClient {
    Ollama(OllamaProvider),
    Anthropic(AnthropicProvider),
}

impl LlmClient {
    /// Build a client from environment variables.
    ///
    /// Reads `LOTHAL_LLM_PROVIDER` ("ollama" or "anthropic") and the
    /// provider-specific vars (`OLLAMA_BASE_URL`, `OLLAMA_MODEL`,
    /// `ANTHROPIC_API_KEY`, `ANTHROPIC_MODEL`).
    pub fn from_env() -> Result<Self, AiError> {
        let provider = std::env::var("LOTHAL_LLM_PROVIDER").unwrap_or_else(|_| "ollama".into());

        match provider.to_lowercase().as_str() {
            "ollama" => {
                let base_url = std::env::var("OLLAMA_BASE_URL")
                    .unwrap_or_else(|_| "http://localhost:11434".into());
                let model = std::env::var("OLLAMA_MODEL")
                    .unwrap_or_else(|_| "gemma3:12b".into());
                Ok(Self::Ollama(OllamaProvider::new(base_url, model)))
            }
            "anthropic" => {
                let api_key = std::env::var("ANTHROPIC_API_KEY").map_err(|_| {
                    AiError::ProviderNotConfigured(
                        "ANTHROPIC_API_KEY must be set when using anthropic provider".into(),
                    )
                })?;
                let model = std::env::var("ANTHROPIC_MODEL")
                    .unwrap_or_else(|_| "claude-sonnet-4-20250514".into());
                Ok(Self::Anthropic(AnthropicProvider::new(api_key, model)))
            }
            other => Err(AiError::ProviderNotConfigured(format!(
                "Unknown provider '{other}'. Use 'ollama' or 'anthropic'."
            ))),
        }
    }

    /// Send a chat completion request and get a text response.
    pub async fn complete(
        &self,
        request: &CompletionRequest,
    ) -> Result<CompletionResponse, AiError> {
        match self {
            Self::Ollama(p) => p.complete(request).await,
            Self::Anthropic(p) => p.complete(request).await,
        }
    }

    /// Send a chat completion request expecting JSON output conforming to the
    /// given schema. The schema is embedded in the system prompt for Ollama and
    /// used as a tool definition for Anthropic.
    pub async fn complete_json(
        &self,
        request: &CompletionRequest,
        schema: &serde_json::Value,
    ) -> Result<serde_json::Value, AiError> {
        match self {
            Self::Ollama(p) => p.complete_json(request, schema).await,
            Self::Anthropic(p) => p.complete_json(request, schema).await,
        }
    }

    /// Check connectivity to the configured provider. Returns the model name
    /// on success.
    pub async fn check_status(&self) -> Result<String, AiError> {
        match self {
            Self::Ollama(p) => p.check_status().await,
            Self::Anthropic(p) => p.check_status().await,
        }
    }

    /// Name of the active provider.
    pub fn provider_name(&self) -> &str {
        match self {
            Self::Ollama(_) => "ollama",
            Self::Anthropic(_) => "anthropic",
        }
    }

    /// Name of the active model.
    pub fn model_name(&self) -> &str {
        match self {
            Self::Ollama(p) => &p.model,
            Self::Anthropic(p) => &p.model,
        }
    }
}
