use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::units::SquareFeet;

/// An area of the property lot (distinct from HVAC `Zone` inside structures).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropertyZone {
    pub id: Uuid,
    pub site_id: Uuid,
    pub name: String,
    pub kind: PropertyZoneKind,
    pub area_sqft: Option<SquareFeet>,
    pub sun_exposure: Option<SunExposure>,
    pub slope: Option<Slope>,
    pub soil_type: Option<crate::ontology::site::SoilType>,
    pub drainage: Option<DrainageType>,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl PropertyZone {
    pub fn new(site_id: Uuid, name: String, kind: PropertyZoneKind) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            site_id,
            name,
            kind,
            area_sqft: None,
            sun_exposure: None,
            slope: None,
            soil_type: None,
            drainage: None,
            notes: None,
            created_at: now,
            updated_at: now,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PropertyZoneKind {
    Lawn,
    Garden,
    Orchard,
    TreeLine,
    Driveway,
    PoolDeck,
    LeachField,
    CoopArea,
    Run,
    Pasture,
    Patio,
    Storage,
    Unstructured,
}

impl std::fmt::Display for PropertyZoneKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = serde_json::to_string(self).unwrap_or_else(|_| "unknown".into());
        write!(f, "{}", s.trim_matches('"'))
    }
}

impl std::str::FromStr for PropertyZoneKind {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let quoted = format!("\"{s}\"");
        serde_json::from_str(&quoted).map_err(|_| format!("unknown property zone kind: {s}"))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SunExposure {
    FullSun,
    PartialSun,
    PartialShade,
    FullShade,
}

impl std::fmt::Display for SunExposure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = serde_json::to_string(self).unwrap_or_else(|_| "unknown".into());
        write!(f, "{}", s.trim_matches('"'))
    }
}

impl std::str::FromStr for SunExposure {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let quoted = format!("\"{s}\"");
        serde_json::from_str(&quoted).map_err(|_| format!("unknown sun exposure: {s}"))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Slope {
    Flat,
    Gentle,
    Moderate,
    Steep,
}

impl std::fmt::Display for Slope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = serde_json::to_string(self).unwrap_or_else(|_| "unknown".into());
        write!(f, "{}", s.trim_matches('"'))
    }
}

impl std::str::FromStr for Slope {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let quoted = format!("\"{s}\"");
        serde_json::from_str(&quoted).map_err(|_| format!("unknown slope: {s}"))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DrainageType {
    Good,
    Moderate,
    Poor,
    Standing,
}

impl std::fmt::Display for DrainageType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = serde_json::to_string(self).unwrap_or_else(|_| "unknown".into());
        write!(f, "{}", s.trim_matches('"'))
    }
}

impl std::str::FromStr for DrainageType {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let quoted = format!("\"{s}\"");
        serde_json::from_str(&quoted).map_err(|_| format!("unknown drainage type: {s}"))
    }
}

/// A restriction on what can be done in certain property zones.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Constraint {
    pub id: Uuid,
    pub site_id: Uuid,
    pub kind: ConstraintKind,
    pub description: String,
    /// Optional GIS geometry (future expansion).
    pub geometry: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl Constraint {
    pub fn new(site_id: Uuid, kind: ConstraintKind, description: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            site_id,
            kind,
            description,
            geometry: None,
            created_at: Utc::now(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConstraintKind {
    LeachField,
    Easement,
    Setback,
    UtilityLine,
    Wellhead,
    FloodZone,
    Other,
}

impl std::fmt::Display for ConstraintKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = serde_json::to_string(self).unwrap_or_else(|_| "unknown".into());
        write!(f, "{}", s.trim_matches('"'))
    }
}

impl std::str::FromStr for ConstraintKind {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let quoted = format!("\"{s}\"");
        serde_json::from_str(&quoted).map_err(|_| format!("unknown constraint kind: {s}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_property_zone_kind_round_trip() {
        let kinds = [
            PropertyZoneKind::Lawn,
            PropertyZoneKind::Garden,
            PropertyZoneKind::LeachField,
            PropertyZoneKind::CoopArea,
            PropertyZoneKind::Unstructured,
        ];
        for kind in kinds {
            let s = kind.to_string();
            let parsed: PropertyZoneKind = s.parse().unwrap();
            assert_eq!(parsed, kind);
        }
    }

    #[test]
    fn test_sun_exposure_round_trip() {
        let exp = SunExposure::PartialShade;
        let s = exp.to_string();
        assert_eq!(s, "partial_shade");
        let parsed: SunExposure = s.parse().unwrap();
        assert_eq!(parsed, exp);
    }

    #[test]
    fn test_constraint_kind_round_trip() {
        let kinds = [
            ConstraintKind::LeachField,
            ConstraintKind::Easement,
            ConstraintKind::UtilityLine,
        ];
        for kind in kinds {
            let s = kind.to_string();
            let parsed: ConstraintKind = s.parse().unwrap();
            assert_eq!(parsed, kind);
        }
    }

    #[test]
    fn test_property_zone_constructor() {
        let site_id = Uuid::new_v4();
        let zone = PropertyZone::new(site_id, "Front Lawn".to_string(), PropertyZoneKind::Lawn);
        assert_eq!(zone.site_id, site_id);
        assert_eq!(zone.name, "Front Lawn");
        assert_eq!(zone.kind, PropertyZoneKind::Lawn);
        assert!(zone.area_sqft.is_none());
    }
}
