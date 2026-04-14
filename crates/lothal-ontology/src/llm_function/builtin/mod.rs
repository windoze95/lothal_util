//! Built-in [`LlmFunction`][super::LlmFunction] impls owned by the ontology
//! crate. These are the LLM functions driven by built-in actions
//! (`run_diagnostic`, `scoped_briefing`, `ingest_bill_pdf`).
//!
//! LLM functions that aren't paired with an ontology action — daily briefings
//! and NILM label classification — live in `lothal-ai::functions` instead.

pub mod diagnostic;
pub mod scoped_briefing;
pub mod bill_extraction;

pub use diagnostic::DiagnosticFunction;
pub use scoped_briefing::ScopedBriefingFunction;
pub use bill_extraction::BillExtractionFunction;
