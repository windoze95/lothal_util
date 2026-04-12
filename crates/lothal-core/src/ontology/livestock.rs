use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::units::Usd;

// ---------------------------------------------------------------------------
// Flock
// ---------------------------------------------------------------------------

/// A group of poultry (or other small livestock) managed as a unit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Flock {
    pub id: Uuid,
    pub site_id: Uuid,
    pub name: String,
    pub breed: String,
    pub bird_count: i32,
    /// The property zone where the coop is located.
    pub coop_zone_id: Option<Uuid>,
    pub date_established: Option<NaiveDate>,
    pub status: FlockStatus,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Flock {
    pub fn new(site_id: Uuid, name: String, breed: String, bird_count: i32) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            site_id,
            name,
            breed,
            bird_count,
            coop_zone_id: None,
            date_established: None,
            status: FlockStatus::Active,
            notes: None,
            created_at: now,
            updated_at: now,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlockStatus {
    Active,
    Retired,
    Deceased,
}

impl std::fmt::Display for FlockStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = serde_json::to_string(self).unwrap_or_else(|_| "unknown".into());
        write!(f, "{}", s.trim_matches('"'))
    }
}

impl std::str::FromStr for FlockStatus {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let quoted = format!("\"{s}\"");
        serde_json::from_str(&quoted).map_err(|_| format!("unknown flock status: {s}"))
    }
}

// ---------------------------------------------------------------------------
// Paddock
// ---------------------------------------------------------------------------

/// A rotational grazing area linked to a flock and a property zone.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Paddock {
    pub id: Uuid,
    pub flock_id: Uuid,
    pub property_zone_id: Uuid,
    pub rotation_order: i32,
    pub last_rested: Option<NaiveDate>,
    pub rest_days_target: Option<i32>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Paddock {
    pub fn new(flock_id: Uuid, property_zone_id: Uuid, rotation_order: i32) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            flock_id,
            property_zone_id,
            rotation_order,
            last_rested: None,
            rest_days_target: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// Days since this paddock was last rested (None if never rested).
    pub fn days_since_rest(&self) -> Option<i64> {
        let last = self.last_rested?;
        let today = Utc::now().date_naive();
        Some((today - last).num_days())
    }
}

// ---------------------------------------------------------------------------
// LivestockLog
// ---------------------------------------------------------------------------

/// A daily log entry for a flock: eggs, feed, water, manure, events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LivestockLog {
    pub id: Uuid,
    pub flock_id: Uuid,
    pub date: NaiveDate,
    pub event_kind: LivestockEventKind,
    pub quantity: Option<f64>,
    pub unit: Option<String>,
    pub cost: Option<Usd>,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl LivestockLog {
    pub fn new(flock_id: Uuid, date: NaiveDate, event_kind: LivestockEventKind) -> Self {
        Self {
            id: Uuid::new_v4(),
            flock_id,
            date,
            event_kind,
            quantity: None,
            unit: None,
            cost: None,
            notes: None,
            created_at: Utc::now(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LivestockEventKind {
    EggCollection,
    FeedConsumed,
    WaterConsumed,
    ManureOutput,
    PredatorIncident,
    Mortality,
    PaddockRotation,
    VetVisit,
    Other,
}

impl std::fmt::Display for LivestockEventKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = serde_json::to_string(self).unwrap_or_else(|_| "unknown".into());
        write!(f, "{}", s.trim_matches('"'))
    }
}

impl std::str::FromStr for LivestockEventKind {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let quoted = format!("\"{s}\"");
        serde_json::from_str(&quoted).map_err(|_| format!("unknown livestock event kind: {s}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_flock_status_round_trip() {
        let vals = [FlockStatus::Active, FlockStatus::Retired, FlockStatus::Deceased];
        for v in vals {
            let s = v.to_string();
            let parsed: FlockStatus = s.parse().unwrap();
            assert_eq!(parsed, v);
        }
    }

    #[test]
    fn test_livestock_event_kind_round_trip() {
        let kinds = [
            LivestockEventKind::EggCollection,
            LivestockEventKind::FeedConsumed,
            LivestockEventKind::PredatorIncident,
            LivestockEventKind::PaddockRotation,
        ];
        for kind in kinds {
            let s = kind.to_string();
            let parsed: LivestockEventKind = s.parse().unwrap();
            assert_eq!(parsed, kind);
        }
    }

    #[test]
    fn test_flock_constructor() {
        let site_id = Uuid::new_v4();
        let flock = Flock::new(site_id, "Layer Girls".to_string(), "Rhode Island Red".to_string(), 6);
        assert_eq!(flock.bird_count, 6);
        assert_eq!(flock.status, FlockStatus::Active);
    }

    #[test]
    fn test_livestock_log_constructor() {
        let flock_id = Uuid::new_v4();
        let date = NaiveDate::from_ymd_opt(2026, 4, 12).unwrap();
        let mut log = LivestockLog::new(flock_id, date, LivestockEventKind::EggCollection);
        log.quantity = Some(5.0);
        log.unit = Some("eggs".to_string());
        assert_eq!(log.quantity.unwrap(), 5.0);
    }
}
