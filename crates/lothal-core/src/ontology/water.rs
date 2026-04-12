use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::units::{Gallons, SquareFeet, Usd};

// ---------------------------------------------------------------------------
// WaterSource
// ---------------------------------------------------------------------------

/// A source of water on the property.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WaterSource {
    pub id: Uuid,
    pub site_id: Uuid,
    pub name: String,
    pub kind: WaterSourceKind,
    pub capacity_gallons: Option<Gallons>,
    pub flow_rate_gpm: Option<f64>,
    pub cost_per_gallon: Option<Usd>,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl WaterSource {
    pub fn new(site_id: Uuid, name: String, kind: WaterSourceKind) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            site_id,
            name,
            kind,
            capacity_gallons: None,
            flow_rate_gpm: None,
            cost_per_gallon: None,
            notes: None,
            created_at: now,
            updated_at: now,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WaterSourceKind {
    Municipal,
    Well,
    Cistern,
    RainwaterCollection,
}

impl std::fmt::Display for WaterSourceKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = serde_json::to_string(self).unwrap_or_else(|_| "unknown".into());
        write!(f, "{}", s.trim_matches('"'))
    }
}

impl std::str::FromStr for WaterSourceKind {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let quoted = format!("\"{s}\"");
        serde_json::from_str(&quoted).map_err(|_| format!("unknown water source kind: {s}"))
    }
}

// ---------------------------------------------------------------------------
// Pool
// ---------------------------------------------------------------------------

/// A swimming pool — simultaneously a thermal mass, evaporation surface,
/// and energy consumer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pool {
    pub id: Uuid,
    pub site_id: Uuid,
    pub name: String,
    pub volume_gallons: Gallons,
    pub surface_area_sqft: Option<SquareFeet>,
    pub pump_device_id: Option<Uuid>,
    pub heater_device_id: Option<Uuid>,
    pub cleaner_device_id: Option<Uuid>,
    pub cover_type: Option<CoverType>,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Pool {
    pub fn new(site_id: Uuid, name: String, volume_gallons: Gallons) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            site_id,
            name,
            volume_gallons,
            surface_area_sqft: None,
            pump_device_id: None,
            heater_device_id: None,
            cleaner_device_id: None,
            cover_type: None,
            notes: None,
            created_at: now,
            updated_at: now,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoverType {
    Manual,
    Automatic,
    Solar,
    Safety,
}

impl std::fmt::Display for CoverType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = serde_json::to_string(self).unwrap_or_else(|_| "unknown".into());
        write!(f, "{}", s.trim_matches('"'))
    }
}

impl std::str::FromStr for CoverType {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let quoted = format!("\"{s}\"");
        serde_json::from_str(&quoted).map_err(|_| format!("unknown cover type: {s}"))
    }
}

// ---------------------------------------------------------------------------
// SepticSystem
// ---------------------------------------------------------------------------

/// The septic system — a slow-burn risk that humans forget and ontologies don't.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SepticSystem {
    pub id: Uuid,
    pub site_id: Uuid,
    pub tank_capacity_gallons: Option<Gallons>,
    /// The property zone containing the leach field.
    pub leach_field_zone_id: Option<Uuid>,
    pub last_pumped: Option<NaiveDate>,
    /// Recommended months between pump-outs (typically 36-60).
    pub pump_interval_months: Option<i32>,
    pub daily_load_estimate_gallons: Option<f64>,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl SepticSystem {
    pub fn new(site_id: Uuid) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            site_id,
            tank_capacity_gallons: None,
            leach_field_zone_id: None,
            last_pumped: None,
            pump_interval_months: None,
            daily_load_estimate_gallons: None,
            notes: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// Compute the estimated next pump date from last_pumped + interval.
    pub fn estimated_next_pump(&self) -> Option<NaiveDate> {
        let last = self.last_pumped?;
        let months = self.pump_interval_months?;
        last.checked_add_months(chrono::Months::new(months as u32))
    }

    /// Days until the next estimated pump (negative if overdue).
    pub fn days_until_pump(&self) -> Option<i64> {
        let next = self.estimated_next_pump()?;
        let today = Utc::now().date_naive();
        Some((next - today).num_days())
    }
}

// ---------------------------------------------------------------------------
// WaterFlow
// ---------------------------------------------------------------------------

/// A directed flow of water between two entities on the property.
///
/// Examples: municipal → house, roof runoff → cistern, cistern → irrigation,
/// rainfall → property, pool evaporation → atmosphere.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WaterFlow {
    pub id: Uuid,
    pub site_id: Uuid,
    pub name: String,
    pub source_type: String,
    pub source_id: Uuid,
    pub sink_type: String,
    pub sink_id: Uuid,
    pub flow_rate_gpm: Option<f64>,
    pub is_active: bool,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl WaterFlow {
    pub fn new(
        site_id: Uuid,
        name: String,
        source_type: String,
        source_id: Uuid,
        sink_type: String,
        sink_id: Uuid,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            site_id,
            name,
            source_type,
            source_id,
            sink_type,
            sink_id,
            flow_rate_gpm: None,
            is_active: true,
            notes: None,
            created_at: Utc::now(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_water_source_kind_round_trip() {
        let kinds = [
            WaterSourceKind::Municipal,
            WaterSourceKind::Well,
            WaterSourceKind::Cistern,
            WaterSourceKind::RainwaterCollection,
        ];
        for kind in kinds {
            let s = kind.to_string();
            let parsed: WaterSourceKind = s.parse().unwrap();
            assert_eq!(parsed, kind);
        }
    }

    #[test]
    fn test_cover_type_round_trip() {
        let types = [
            CoverType::Manual,
            CoverType::Automatic,
            CoverType::Solar,
            CoverType::Safety,
        ];
        for t in types {
            let s = t.to_string();
            let parsed: CoverType = s.parse().unwrap();
            assert_eq!(parsed, t);
        }
    }

    #[test]
    fn test_pool_constructor() {
        let site_id = Uuid::new_v4();
        let pool = Pool::new(site_id, "Main Pool".to_string(), Gallons::new(15000.0));
        assert_eq!(pool.site_id, site_id);
        assert_eq!(pool.volume_gallons.value(), 15000.0);
        assert!(pool.cover_type.is_none());
    }

    #[test]
    fn test_septic_estimated_next_pump() {
        let site_id = Uuid::new_v4();
        let mut septic = SepticSystem::new(site_id);

        // No data yet
        assert!(septic.estimated_next_pump().is_none());

        septic.last_pumped = Some(NaiveDate::from_ymd_opt(2025, 3, 15).unwrap());
        septic.pump_interval_months = Some(36);

        let next = septic.estimated_next_pump().unwrap();
        assert_eq!(next, NaiveDate::from_ymd_opt(2028, 3, 15).unwrap());
    }

    #[test]
    fn test_water_flow_constructor() {
        let site_id = Uuid::new_v4();
        let src_id = Uuid::new_v4();
        let sink_id = Uuid::new_v4();
        let flow = WaterFlow::new(
            site_id,
            "Municipal to house".to_string(),
            "water_source".to_string(),
            src_id,
            "structure".to_string(),
            sink_id,
        );
        assert!(flow.is_active);
        assert_eq!(flow.source_type, "water_source");
    }
}
