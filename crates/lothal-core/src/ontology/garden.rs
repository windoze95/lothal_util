use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::units::{CubicFeet, Gallons, Pounds, SquareFeet};

// ---------------------------------------------------------------------------
// GardenBed
// ---------------------------------------------------------------------------

/// A garden bed — raised, in-ground, or container.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GardenBed {
    pub id: Uuid,
    pub site_id: Uuid,
    pub property_zone_id: Option<Uuid>,
    pub name: String,
    pub area_sqft: Option<SquareFeet>,
    pub bed_type: BedType,
    pub soil_amendments: Option<String>,
    /// Which water source irrigates this bed.
    pub irrigation_source_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl GardenBed {
    pub fn new(site_id: Uuid, name: String, bed_type: BedType) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            site_id,
            property_zone_id: None,
            name,
            area_sqft: None,
            bed_type,
            soil_amendments: None,
            irrigation_source_id: None,
            created_at: now,
            updated_at: now,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BedType {
    InGround,
    Raised,
    Container,
    Hydroponic,
    Other,
}

impl std::fmt::Display for BedType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = serde_json::to_string(self).unwrap_or_else(|_| "unknown".into());
        write!(f, "{}", s.trim_matches('"'))
    }
}

impl std::str::FromStr for BedType {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let quoted = format!("\"{s}\"");
        serde_json::from_str(&quoted).map_err(|_| format!("unknown bed type: {s}"))
    }
}

// ---------------------------------------------------------------------------
// Planting
// ---------------------------------------------------------------------------

/// A crop planted in a garden bed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Planting {
    pub id: Uuid,
    pub bed_id: Uuid,
    pub crop: String,
    pub variety: Option<String>,
    pub date_planted: NaiveDate,
    pub date_harvested: Option<NaiveDate>,
    pub yield_lbs: Option<Pounds>,
    pub water_consumed_gallons: Option<Gallons>,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl Planting {
    pub fn new(bed_id: Uuid, crop: String, date_planted: NaiveDate) -> Self {
        Self {
            id: Uuid::new_v4(),
            bed_id,
            crop,
            variety: None,
            date_planted,
            date_harvested: None,
            yield_lbs: None,
            water_consumed_gallons: None,
            notes: None,
            created_at: Utc::now(),
        }
    }

    /// Days from planting to harvest (None if not yet harvested).
    pub fn days_to_harvest(&self) -> Option<i64> {
        let harvested = self.date_harvested?;
        Some((harvested - self.date_planted).num_days())
    }
}

// ---------------------------------------------------------------------------
// CompostPile
// ---------------------------------------------------------------------------

/// A compost pile — tracks inputs (kitchen scraps, manure, yard waste)
/// and outputs (finished compost applied to beds).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompostPile {
    pub id: Uuid,
    pub site_id: Uuid,
    pub property_zone_id: Option<Uuid>,
    pub name: String,
    pub capacity_cuft: Option<CubicFeet>,
    pub current_volume_cuft: Option<CubicFeet>,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl CompostPile {
    pub fn new(site_id: Uuid, name: String) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            site_id,
            property_zone_id: None,
            name,
            capacity_cuft: None,
            current_volume_cuft: None,
            notes: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// Percentage full (None if capacity unknown).
    pub fn fill_pct(&self) -> Option<f64> {
        let cap = self.capacity_cuft?.value();
        let cur = self.current_volume_cuft?.value();
        if cap > 0.0 {
            Some((cur / cap) * 100.0)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bed_type_round_trip() {
        let types = [
            BedType::InGround,
            BedType::Raised,
            BedType::Container,
            BedType::Hydroponic,
            BedType::Other,
        ];
        for t in types {
            let s = t.to_string();
            let parsed: BedType = s.parse().unwrap();
            assert_eq!(parsed, t);
        }
    }

    #[test]
    fn test_garden_bed_constructor() {
        let site_id = Uuid::new_v4();
        let bed = GardenBed::new(site_id, "Raised Bed #1".to_string(), BedType::Raised);
        assert_eq!(bed.bed_type, BedType::Raised);
        assert!(bed.area_sqft.is_none());
    }

    #[test]
    fn test_planting_days_to_harvest() {
        let bed_id = Uuid::new_v4();
        let mut p = Planting::new(
            bed_id,
            "Tomato".to_string(),
            NaiveDate::from_ymd_opt(2026, 3, 15).unwrap(),
        );
        assert!(p.days_to_harvest().is_none());

        p.date_harvested = Some(NaiveDate::from_ymd_opt(2026, 7, 1).unwrap());
        assert_eq!(p.days_to_harvest().unwrap(), 108);
    }

    #[test]
    fn test_compost_fill_pct() {
        let site_id = Uuid::new_v4();
        let mut pile = CompostPile::new(site_id, "Main Pile".to_string());
        assert!(pile.fill_pct().is_none());

        pile.capacity_cuft = Some(CubicFeet::new(27.0)); // 1 cubic yard
        pile.current_volume_cuft = Some(CubicFeet::new(16.2));
        assert!((pile.fill_pct().unwrap() - 60.0).abs() < 0.1);
    }
}
