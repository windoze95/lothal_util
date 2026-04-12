use serde::Serialize;
use uuid::Uuid;

/// A domain type that can be described as an ontology object.
pub trait Describe: Serialize {
    const KIND: &'static str;

    fn id(&self) -> Uuid;
    fn site_id(&self) -> Option<Uuid>;
    fn display_name(&self) -> String;

    fn properties(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }

    fn uri(&self) -> crate::ObjectUri {
        crate::ObjectUri::new(Self::KIND, self.id())
    }
}

/// Persisted representation of an ontology object.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ObjectRecord {
    pub kind: String,
    pub id: Uuid,
    pub display_name: String,
    pub site_id: Option<Uuid>,
    pub properties: sqlx::types::Json<serde_json::Value>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub deleted_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Lightweight reference to an object by kind + id.
#[derive(Debug, Clone)]
pub struct ObjectRef {
    pub kind: String,
    pub id: Uuid,
}

impl ObjectRef {
    pub fn new(kind: impl Into<String>, id: Uuid) -> Self {
        Self {
            kind: kind.into(),
            id,
        }
    }
}
