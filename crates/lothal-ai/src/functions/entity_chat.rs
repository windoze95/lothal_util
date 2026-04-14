//! `entity_chat` — single-round LLM call for the entity-scoped chat loop.
//!
//! The chat handler in `lothal-web` owns the multi-round tool-use loop. Each
//! round it invokes this function, which records one `llm_calls` trace row
//! per round with a stable `prompt_hash` (the system prompt is fixed; the
//! entity-specific context lives in the first user message so prompt
//! versioning stays meaningful).
//!
//! The function returns the raw content blocks — a mix of `text` and
//! `tool_use` — so the caller can dispatch tools between rounds without
//! having to know about the underlying provider shape.

use async_trait::async_trait;
use serde_json::json;

use lothal_ontology::llm_function::{
    ChatInvokeRequest, LlmFunction, LlmFunctionCtx, LlmFunctionError, LlmFunctionOutput,
    InvokeResponse, ModelTier,
};

pub struct EntityChatFunction;

const MAX_OUTPUT_TOKENS: u32 = 2048;

/// Stable system prompt. The entity-specific context (kind, id, display
/// name) is injected as the first user message instead of being baked into
/// this prompt so `sha256(system_prompt)` stays constant across chats.
const SYSTEM_PROMPT: &str = "\
You are Lothal, a homestead operations agent. Conversations are scoped to a \
single ontology entity; the user will tell you which one in their first \
message. You have tools to read the ontology (get_object, neighbors, events, \
timeline, search) and to invoke actions (run_action). Use them when a \
question requires data beyond your context. Be concise and actionable.";

#[async_trait]
impl LlmFunction for EntityChatFunction {
    fn name(&self) -> &'static str {
        "entity_chat"
    }

    fn description(&self) -> &'static str {
        "Single round of the entity-scoped chat tool-use loop."
    }

    fn tier(&self) -> ModelTier {
        ModelTier::Frontier
    }

    fn system_prompt(&self) -> &str {
        SYSTEM_PROMPT
    }

    fn max_tokens(&self) -> u32 {
        MAX_OUTPUT_TOKENS
    }

    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "required": ["messages", "tools"],
            "properties": {
                "messages": {"type": "array"},
                "tools":    {"type": "array"}
            }
        })
    }

    fn output_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "required": ["content"],
            "properties": {
                "content": {"type": "array"}
            }
        })
    }

    async fn run(
        &self,
        ctx: &LlmFunctionCtx,
        input: serde_json::Value,
    ) -> Result<LlmFunctionOutput, LlmFunctionError> {
        let invoker = ctx
            .invoker
            .as_ref()
            .ok_or(LlmFunctionError::NoInvoker)?;

        let messages = input
            .get("messages")
            .and_then(|v| v.as_array())
            .ok_or_else(|| LlmFunctionError::InvalidInput("missing `messages` array".into()))?
            .clone();
        let tools = input
            .get("tools")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        let req = ChatInvokeRequest {
            tier: self.tier(),
            system: SYSTEM_PROMPT.to_string(),
            messages,
            tools,
            max_tokens: self.max_tokens(),
        };

        let response = invoker
            .chat_invoke(&req)
            .await
            .map_err(LlmFunctionError::Other)?;

        let content_array = serde_json::Value::Array(response.content.clone());

        Ok(LlmFunctionOutput {
            output: json!({ "content": content_array.clone() }),
            response: InvokeResponse {
                content: content_array,
                model: response.model,
                tokens_in: response.tokens_in,
                tokens_out: response.tokens_out,
            },
        })
    }
}
