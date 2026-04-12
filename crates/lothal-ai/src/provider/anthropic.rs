use reqwest::Client;
use serde_json::json;

use super::{CompletionRequest, CompletionResponse, Role};
use crate::AiError;

const ANTHROPIC_API_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";

/// Anthropic Claude provider using the Messages API directly.
pub struct AnthropicProvider {
    client: Client,
    api_key: String,
    pub model: String,
}

impl AnthropicProvider {
    pub fn new(api_key: String, model: String) -> Self {
        Self {
            client: Client::new(),
            api_key,
            model,
        }
    }

    pub async fn complete(
        &self,
        request: &CompletionRequest,
    ) -> Result<CompletionResponse, AiError> {
        let messages = build_anthropic_messages(request);

        let body = json!({
            "model": self.model,
            "max_tokens": request.max_tokens,
            "temperature": request.temperature,
            "system": request.system,
            "messages": messages,
        });

        let resp = self
            .client
            .post(ANTHROPIC_API_URL)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(AiError::LlmRequest(format!("Anthropic {status}: {text}")));
        }

        let data: serde_json::Value = resp.json().await?;
        parse_anthropic_response(&data, &self.model)
    }

    /// Use Claude's tool_use mechanism for structured output.
    ///
    /// Defines a single tool whose `input_schema` is the desired JSON schema,
    /// then forces `tool_choice` to that tool. Claude returns the structured
    /// data as tool input.
    pub async fn complete_json(
        &self,
        request: &CompletionRequest,
        schema: &serde_json::Value,
    ) -> Result<serde_json::Value, AiError> {
        let messages = build_anthropic_messages(request);

        let tool_name = "structured_output";
        let body = json!({
            "model": self.model,
            "max_tokens": request.max_tokens,
            "temperature": request.temperature,
            "system": request.system,
            "messages": messages,
            "tools": [{
                "name": tool_name,
                "description": "Return structured data matching the required schema.",
                "input_schema": schema,
            }],
            "tool_choice": {
                "type": "tool",
                "name": tool_name,
            },
        });

        let resp = self
            .client
            .post(ANTHROPIC_API_URL)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(AiError::LlmRequest(format!("Anthropic {status}: {text}")));
        }

        let data: serde_json::Value = resp.json().await?;

        // Extract tool_use input from the response content blocks.
        let content = data["content"]
            .as_array()
            .ok_or_else(|| AiError::LlmResponse("Missing content array".into()))?;

        for block in content {
            if block["type"] == "tool_use" {
                return Ok(block["input"].clone());
            }
        }

        Err(AiError::LlmResponse(
            "No tool_use block in Anthropic response".into(),
        ))
    }

    pub async fn check_status(&self) -> Result<String, AiError> {
        // Send a minimal request to verify the API key works.
        let body = json!({
            "model": self.model,
            "max_tokens": 16,
            "messages": [{"role": "user", "content": "ping"}],
        });

        let resp = self
            .client
            .post(ANTHROPIC_API_URL)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(AiError::LlmRequest(format!("Anthropic {status}: {text}")));
        }

        Ok(format!("Anthropic OK — model: {}", self.model))
    }
}

fn build_anthropic_messages(request: &CompletionRequest) -> Vec<serde_json::Value> {
    request
        .messages
        .iter()
        .map(|msg| {
            let role = match msg.role {
                Role::User | Role::System => "user",
                Role::Assistant => "assistant",
            };
            json!({
                "role": role,
                "content": msg.content,
            })
        })
        .collect()
}

fn parse_anthropic_response(
    data: &serde_json::Value,
    model: &str,
) -> Result<CompletionResponse, AiError> {
    let content = data["content"]
        .as_array()
        .and_then(|arr| arr.iter().find(|b| b["type"] == "text"))
        .and_then(|b| b["text"].as_str())
        .ok_or_else(|| AiError::LlmResponse(format!("Missing text in response: {data}")))?
        .to_string();

    let input_tokens = data["usage"]["input_tokens"].as_u64().map(|n| n as u32);
    let output_tokens = data["usage"]["output_tokens"].as_u64().map(|n| n as u32);

    Ok(CompletionResponse {
        content,
        model: data["model"].as_str().unwrap_or(model).to_string(),
        input_tokens,
        output_tokens,
    })
}
