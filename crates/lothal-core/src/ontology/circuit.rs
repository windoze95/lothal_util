use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// An electrical panel in a structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Panel {
    pub id: Uuid,
    pub structure_id: Uuid,
    pub name: String,
    pub amperage: Option<i32>,
    pub is_main: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Panel {
    pub fn new(structure_id: Uuid, name: String) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            structure_id,
            name,
            amperage: None,
            is_main: true,
            created_at: now,
            updated_at: now,
        }
    }
}

/// A breaker circuit within a panel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Circuit {
    pub id: Uuid,
    pub panel_id: Uuid,
    pub breaker_number: i32,
    pub label: String,
    pub amperage: i32,
    pub is_double_pole: bool,
    pub device_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Circuit {
    pub fn new(panel_id: Uuid, breaker_number: i32, label: String, amperage: i32) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            panel_id,
            breaker_number,
            label,
            amperage,
            is_double_pole: false,
            device_id: None,
            created_at: now,
            updated_at: now,
        }
    }
}
