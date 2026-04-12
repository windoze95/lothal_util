use reqwest::Client;
use serde_json::json;

use super::{CompletionRequest, CompletionResponse, Role};
use crate::AiError;

/// Ollama provider using the OpenAI-compatible `/v1/chat/completions` endpoint.
pub struct OllamaProvider {
    client: Client,
    base_url: String,
    pub model: String,
}

impl OllamaProvider {
    pub fn new(base_url: String, model: String) -> Self {
        Self {
            client: Client::new(),
            base_url,
            model,
        }
    }

    pub async fn complete(
        &self,
        request: &CompletionRequest,
    ) -> Result<CompletionResponse, AiError> {
        let messages = build_messages(request);

        let body = json!({
            "model": self.model,
            "messages": messages,
            "max_tokens": request.max_tokens,
            "temperature": request.temperature,
            "stream": false,
        });

        let resp = self
            .client
            .post(format!("{}/v1/chat/completions", self.base_url))
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(AiError::LlmRequest(format!("Ollama {status}: {text}")));
        }

        let data: serde_json::Value = resp.json().await?;
        parse_openai_response(&data, &self.model)
    }

    pub async fn complete_json(
        &self,
        request: &CompletionRequest,
        schema: &serde_json::Value,
    ) -> Result<serde_json::Value, AiError> {
        // Ollama supports JSON mode via response_format. We include the schema
        // in the system prompt so the model knows the expected structure.
        let schema_instruction = format!(
            "\n\nYou MUST respond with valid JSON matching this schema:\n```json\n{}\n```",
            serde_json::to_string_pretty(schema).unwrap_or_default()
        );

        let augmented = CompletionRequest {
            system: format!("{}{}", request.system, schema_instruction),
            messages: request.messages.clone(),
            max_tokens: request.max_tokens,
            temperature: request.temperature,
        };

        let messages = build_messages(&augmented);

        let body = json!({
            "model": self.model,
            "messages": messages,
            "max_tokens": request.max_tokens,
            "temperature": request.temperature,
            "stream": false,
            "response_format": { "type": "json_object" },
        });

        let resp = self
            .client
            .post(format!("{}/v1/chat/completions", self.base_url))
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(AiError::LlmRequest(format!("Ollama {status}: {text}")));
        }

        let data: serde_json::Value = resp.json().await?;
        let response = parse_openai_response(&data, &self.model)?;

        serde_json::from_str(&response.content)
            .map_err(|e| AiError::LlmResponse(format!("Invalid JSON from Ollama: {e}")))
    }

    pub async fn check_status(&self) -> Result<String, AiError> {
        let resp = self
            .client
            .get(format!("{}/api/tags", self.base_url))
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(AiError::LlmRequest(format!(
                "Ollama not reachable at {}",
                self.base_url
            )));
        }

        let data: serde_json::Value = resp.json().await?;
        let models = data["models"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|m| m["name"].as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_else(|| "unknown".into());

        Ok(format!("Ollama OK — model: {}, available: [{}]", self.model, models))
    }
}

fn build_messages(request: &CompletionRequest) -> Vec<serde_json::Value> {
    let mut messages = Vec::new();

    if !request.system.is_empty() {
        messages.push(json!({
            "role": "system",
            "content": request.system,
        }));
    }

    for msg in &request.messages {
        let role = match msg.role {
            Role::System => "system",
            Role::User => "user",
            Role::Assistant => "assistant",
        };
        messages.push(json!({
            "role": role,
            "content": msg.content,
        }));
    }

    messages
}

fn parse_openai_response(
    data: &serde_json::Value,
    model: &str,
) -> Result<CompletionResponse, AiError> {
    let content = data["choices"][0]["message"]["content"]
        .as_str()
        .ok_or_else(|| {
            AiError::LlmResponse(format!("Missing content in Ollama response: {data}"))
        })?
        .to_string();

    let input_tokens = data["usage"]["prompt_tokens"].as_u64().map(|n| n as u32);
    let output_tokens = data["usage"]["completion_tokens"]
        .as_u64()
        .map(|n| n as u32);

    Ok(CompletionResponse {
        content,
        model: data["model"].as_str().unwrap_or(model).to_string(),
        input_tokens,
        output_tokens,
    })
}
