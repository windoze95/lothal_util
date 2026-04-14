//! [`LlmFunction`][lothal_ontology::LlmFunction] impls that live in
//! `lothal-ai` rather than the ontology crate because they drive
//! LLM-only features (daily briefings, NILM classification) rather than
//! being paired with an ontology action.

pub mod calm_briefing;
pub mod diagnose_briefing;
pub mod entity_chat;
pub mod nilm_label;

pub use calm_briefing::CalmBriefingFunction;
pub use diagnose_briefing::DiagnoseBriefingFunction;
pub use entity_chat::EntityChatFunction;
pub use nilm_label::NilmLabelFunction;

use std::sync::Arc;

use lothal_ontology::llm_function::{LlmFunctionRegistry, LlmInvoker};

/// Build a fully-populated [`LlmFunctionRegistry`] with every default
/// function pre-registered and the given invoker wired in.
///
/// This is the canonical way the web/daemon/cli paths obtain a registry.
pub fn default_registry(invoker: Arc<dyn LlmInvoker>) -> Arc<LlmFunctionRegistry> {
    let mut reg = LlmFunctionRegistry::new().with_invoker(invoker);

    // Ontology-owned builtins (paired with Actions).
    reg.register(Arc::new(
        lothal_ontology::llm_function::builtin::DiagnosticFunction,
    ));
    reg.register(Arc::new(
        lothal_ontology::llm_function::builtin::ScopedBriefingFunction,
    ));
    reg.register(Arc::new(
        lothal_ontology::llm_function::builtin::BillExtractionFunction,
    ));

    // `lothal-ai`-native functions (not paired with an ontology action).
    reg.register(Arc::new(CalmBriefingFunction));
    reg.register(Arc::new(DiagnoseBriefingFunction));
    reg.register(Arc::new(EntityChatFunction));
    reg.register(Arc::new(NilmLabelFunction));

    Arc::new(reg)
}
