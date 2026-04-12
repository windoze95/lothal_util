use uuid::Uuid;

use crate::{Describe, ObjectRef};

/// Persisted representation of an ontology event.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct EventRecord {
    pub time: chrono::DateTime<chrono::Utc>,
    pub id: Uuid,
    pub kind: String,
    pub site_id: Option<Uuid>,
    pub subjects: sqlx::types::Json<Vec<serde_json::Value>>,
    pub summary: String,
    pub severity: Option<String>,
    pub properties: sqlx::types::Json<serde_json::Value>,
    pub source: String,
}

/// Specification for emitting an event.
#[derive(Debug, Clone)]
pub struct EventSpec {
    pub kind: String,
    pub site_id: Option<Uuid>,
    pub subjects: Vec<ObjectRef>,
    pub summary: String,
    pub severity: Option<String>,
    pub properties: serde_json::Value,
    pub source: String,
}

impl EventSpec {
    /// Build a `{kind}_registered` event for a freshly-registered object.
    pub fn record_registered<T: Describe>(obj: &T, source: impl Into<String>) -> Self {
        Self {
            kind: format!("{}_registered", T::KIND),
            site_id: obj.site_id(),
            subjects: vec![ObjectRef::new(T::KIND, obj.id())],
            summary: format!("{} registered: {}", T::KIND, obj.display_name()),
            severity: None,
            properties: serde_json::Value::Null,
            source: source.into(),
        }
    }
}
