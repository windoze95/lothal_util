//! Concrete [`LlmInvoker`] impl that bridges `lothal-ontology`'s declarative
//! function layer to `lothal-ai`'s provider pool.
//!
//! Every LLM call issued from inside an `LlmFunction::run` body flows through
//! [`LlmClientInvoker::invoke`], which turns an [`InvokeRequest`] into a
//! [`CompletionRequest`] and dispatches to the wrapped [`LlmClient`]. The
//! returned [`InvokeResponse`] carries the concrete model name and token
//! counts so the `LlmFunctionRegistry` can record them on the trace row.
//!
//! Phase 2 holds a single [`LlmClient`]; Phase 3 will replace this with a
//! tier-aware dispatcher that picks between a local-tier provider (Ollama)
//! and a frontier-tier provider (Anthropic) based on
//! [`InvokeRequest::tier`].

use async_trait::async_trait;

use lothal_ontology::llm_function::{InvokeRequest, InvokeResponse, LlmInvoker};

use crate::provider::{CompletionRequest, LlmClient, Message, Role};

/// Wraps a single [`LlmClient`] and exposes it as an [`LlmInvoker`].
///
/// In Phase 2 the tier is recorded but not used to route — every call goes to
/// the wrapped client regardless of tier. Phase 3 will introduce a
/// tier-aware variant that carries a local + frontier client and routes on
/// `req.tier`.
pub struct LlmClientInvoker {
    client: LlmClient,
}

impl LlmClientInvoker {
    pub fn new(client: LlmClient) -> Self {
        Self { client }
    }
}

#[async_trait]
impl LlmInvoker for LlmClientInvoker {
    async fn invoke(&self, req: &InvokeRequest) -> Result<InvokeResponse, anyhow::Error> {
        let completion = CompletionRequest {
            system: req.system.clone(),
            messages: vec![Message {
                role: Role::User,
                content: req.user.clone(),
            }],
            max_tokens: req.max_tokens,
            temperature: default_temperature(req),
            budget_tokens: req.budget_tokens,
        };

        let (content, model, tokens_in, tokens_out) = match &req.json_schema {
            Some(schema) => {
                // Structured-output path. `LlmClient::complete_json` discards
                // the `CompletionResponse` metadata today, so for accurate
                // trace rows we separately hit `complete` first when we need
                // token counts — but that's a Phase-3 concern. For now, the
                // JSON result has no model/token metadata and we fall back to
                // the client's configured model name + `None` counts.
                let value = self.client.complete_json(&completion, schema).await?;
                let model = self.client.model_name().to_string();
                (value, model, None, None)
            }
            None => {
                let response = self.client.complete(&completion).await?;
                let content = serde_json::Value::String(response.content);
                (
                    content,
                    response.model,
                    response.input_tokens,
                    response.output_tokens,
                )
            }
        };

        Ok(InvokeResponse {
            content,
            model,
            tokens_in,
            tokens_out,
        })
    }
}

/// Pick a temperature that respects extended-thinking mode.
///
/// When `budget_tokens` is set, Anthropic requires `temperature = 1`. For
/// non-thinking calls we default to `0.3` which matches the existing
/// briefing behaviour and is the right low-variance default for extraction /
/// classification work.
fn default_temperature(req: &InvokeRequest) -> f32 {
    if req.budget_tokens.is_some() {
        1.0
    } else {
        0.3
    }
}
