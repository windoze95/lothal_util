use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::units::Usd;

/// A maintenance event on a device or structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaintenanceEvent {
    pub id: Uuid,
    pub target: MaintenanceTarget,
    pub date: NaiveDate,
    pub event_type: MaintenanceType,
    pub description: String,
    pub cost: Option<Usd>,
    pub provider: Option<String>,
    pub next_due: Option<NaiveDate>,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl MaintenanceEvent {
    pub fn new(
        target: MaintenanceTarget,
        date: NaiveDate,
        event_type: MaintenanceType,
        description: String,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            target,
            date,
            event_type,
            description,
            cost: None,
            provider: None,
            next_due: None,
            notes: None,
            created_at: Utc::now(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "id")]
pub enum MaintenanceTarget {
    Device(Uuid),
    Structure(Uuid),
    PropertyZone(Uuid),
    Pool(Uuid),
    Tree(Uuid),
    SepticSystem(Uuid),
}

impl MaintenanceTarget {
    pub fn target_type(&self) -> &'static str {
        match self {
            Self::Device(_) => "device",
            Self::Structure(_) => "structure",
            Self::PropertyZone(_) => "property_zone",
            Self::Pool(_) => "pool",
            Self::Tree(_) => "tree",
            Self::SepticSystem(_) => "septic_system",
        }
    }

    pub fn target_id(&self) -> Uuid {
        match self {
            Self::Device(id)
            | Self::Structure(id)
            | Self::PropertyZone(id)
            | Self::Pool(id)
            | Self::Tree(id)
            | Self::SepticSystem(id) => *id,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaintenanceType {
    Inspection,
    Repair,
    Replacement,
    Cleaning,
    FilterChange,
    Tune,
    SepticPump,
    PoolService,
    PestControl,
    CoopCleaning,
    PaddockRotation,
    CompostTurning,
    GardenAmendment,
    TreeTrimming,
    TreeRemoval,
    Other,
}

impl std::fmt::Display for MaintenanceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Inspection => write!(f, "Inspection"),
            Self::Repair => write!(f, "Repair"),
            Self::Replacement => write!(f, "Replacement"),
            Self::Cleaning => write!(f, "Cleaning"),
            Self::FilterChange => write!(f, "Filter Change"),
            Self::Tune => write!(f, "Tune-Up"),
            Self::SepticPump => write!(f, "Septic Pump"),
            Self::PoolService => write!(f, "Pool Service"),
            Self::PestControl => write!(f, "Pest Control"),
            Self::CoopCleaning => write!(f, "Coop Cleaning"),
            Self::PaddockRotation => write!(f, "Paddock Rotation"),
            Self::CompostTurning => write!(f, "Compost Turning"),
            Self::GardenAmendment => write!(f, "Garden Amendment"),
            Self::TreeTrimming => write!(f, "Tree Trimming"),
            Self::TreeRemoval => write!(f, "Tree Removal"),
            Self::Other => write!(f, "Other"),
        }
    }
}
