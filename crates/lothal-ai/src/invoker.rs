//! Concrete [`LlmInvoker`] impl that bridges `lothal-ontology`'s declarative
//! function layer to `lothal-ai`'s provider pool.
//!
//! Every LLM call issued from inside an `LlmFunction::run` body flows through
//! [`LlmClientInvoker::invoke`], which turns an [`InvokeRequest`] into a
//! [`CompletionRequest`] and dispatches to the wrapped [`LlmClient`] on the
//! tier declared by the function. The returned [`InvokeResponse`] carries
//! the concrete model name and token counts so the `LlmFunctionRegistry` can
//! record them on the trace row.

use async_trait::async_trait;

use lothal_ontology::llm_function::{InvokeRequest, InvokeResponse, LlmInvoker};

use crate::provider::{CompletionRequest, LlmClient, Message, Role};

/// Wraps an [`LlmClient`] (two-tier: local + frontier) and exposes it as an
/// [`LlmInvoker`]. Each [`InvokeRequest::tier`] drives which provider
/// actually runs the call.
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
                // `complete_json_for_tier` returns only the parsed JSON; the
                // CompletionResponse metadata isn't threaded through today,
                // so the trace row's model comes from the resolved provider
                // and `tokens_in`/`tokens_out` stay NULL. Text-mode calls
                // below get the richer shape.
                let value = self
                    .client
                    .complete_json_for_tier(req.tier, &completion, schema)
                    .await?;
                let model = self
                    .client
                    .model_name_for_tier(req.tier)
                    .unwrap_or("unknown")
                    .to_string();
                (value, model, None, None)
            }
            None => {
                let response = self.client.complete_for_tier(req.tier, &completion).await?;
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
