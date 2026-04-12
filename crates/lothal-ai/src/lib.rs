pub mod provider;
pub mod extract;
pub mod briefing;
pub mod mcp;
pub mod nilm;

#[derive(Debug, thiserror::Error)]
pub enum AiError {
    #[error("LLM request failed: {0}")]
    LlmRequest(String),
    #[error("LLM returned invalid response: {0}")]
    LlmResponse(String),
    #[error("JSON parsing error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Validation failed: {0}")]
    Validation(String),
    #[error("Validation failed after {attempts} retries: {message}")]
    ValidationExhausted { attempts: u32, message: String },
    #[error("Provider not configured: {0}")]
    ProviderNotConfigured(String),
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("PDF extraction failed: {0}")]
    PdfExtraction(String),
    #[error("IMAP error: {0}")]
    Imap(String),
    #[error("MCP protocol error: {0}")]
    Mcp(String),
}
