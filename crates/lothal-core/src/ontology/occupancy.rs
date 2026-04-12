use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// An occupancy event — who's home and when.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OccupancyEvent {
    pub id: Uuid,
    pub site_id: Uuid,
    pub start_time: DateTime<Utc>,
    pub end_time: Option<DateTime<Utc>>,
    pub occupant_count: i32,
    pub status: OccupancyStatus,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl OccupancyEvent {
    pub fn new(site_id: Uuid, status: OccupancyStatus, occupant_count: i32) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            site_id,
            start_time: now,
            end_time: None,
            occupant_count,
            status,
            notes: None,
            created_at: now,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OccupancyStatus {
    Home,
    Away,
    Vacation,
    Guests,
    WorkFromHome,
}

impl std::fmt::Display for OccupancyStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Home => write!(f, "home"),
            Self::Away => write!(f, "away"),
            Self::Vacation => write!(f, "vacation"),
            Self::Guests => write!(f, "guests"),
            Self::WorkFromHome => write!(f, "work from home"),
        }
    }
}
