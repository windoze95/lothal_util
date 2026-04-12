use uuid::Uuid;

use crate::ObjectRef;

/// Persisted representation of a typed link between two objects.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct LinkRecord {
    pub id: Uuid,
    pub kind: String,
    pub src_kind: String,
    pub src_id: Uuid,
    pub dst_kind: String,
    pub dst_id: Uuid,
    pub valid_from: chrono::DateTime<chrono::Utc>,
    pub valid_until: Option<chrono::DateTime<chrono::Utc>>,
    pub properties: sqlx::types::Json<serde_json::Value>,
}

/// Specification for inserting / upserting a link.
#[derive(Debug, Clone)]
pub struct LinkSpec {
    pub kind: String,
    pub src: ObjectRef,
    pub dst: ObjectRef,
    pub properties: serde_json::Value,
}

impl LinkSpec {
    pub fn new(kind: impl Into<String>, src: ObjectRef, dst: ObjectRef) -> Self {
        Self {
            kind: kind.into(),
            src,
            dst,
            properties: serde_json::Value::Null,
        }
    }

    pub fn with_properties(mut self, v: serde_json::Value) -> Self {
        self.properties = v;
        self
    }
}
