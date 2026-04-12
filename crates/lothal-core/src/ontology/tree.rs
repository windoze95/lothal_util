use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::units::Usd;

/// A significant tree on the property.
///
/// Trees affect HVAC load (shade), pool maintenance (leaf drop), wildlife,
/// and property value. Modeling them enables shade analysis, removal ROI,
/// and cross-system impact assessment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tree {
    pub id: Uuid,
    pub site_id: Uuid,
    pub property_zone_id: Option<Uuid>,
    pub species: String,
    pub common_name: Option<String>,
    pub canopy_radius_ft: Option<f64>,
    pub height_ft: Option<f64>,
    pub health: TreeHealth,
    /// Distance to the nearest structure in feet.
    pub distance_to_structure_ft: Option<f64>,
    /// Cardinal direction of shade cast (e.g. "NW", "S").
    pub shade_direction: Option<String>,
    /// Estimated annual cooling savings from this tree's shade.
    pub estimated_cooling_value_usd: Option<Usd>,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Tree {
    pub fn new(site_id: Uuid, species: String) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            site_id,
            property_zone_id: None,
            species,
            common_name: None,
            canopy_radius_ft: None,
            height_ft: None,
            health: TreeHealth::Unknown,
            distance_to_structure_ft: None,
            shade_direction: None,
            estimated_cooling_value_usd: None,
            notes: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// Approximate canopy area in square feet.
    pub fn canopy_area_sqft(&self) -> Option<f64> {
        let r = self.canopy_radius_ft?;
        Some(std::f64::consts::PI * r * r)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TreeHealth {
    Excellent,
    Good,
    Fair,
    Poor,
    Dead,
    Unknown,
}

impl std::fmt::Display for TreeHealth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = serde_json::to_string(self).unwrap_or_else(|_| "unknown".into());
        write!(f, "{}", s.trim_matches('"'))
    }
}

impl std::str::FromStr for TreeHealth {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let quoted = format!("\"{s}\"");
        serde_json::from_str(&quoted).map_err(|_| format!("unknown tree health: {s}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tree_health_round_trip() {
        let vals = [
            TreeHealth::Excellent,
            TreeHealth::Good,
            TreeHealth::Fair,
            TreeHealth::Poor,
            TreeHealth::Dead,
            TreeHealth::Unknown,
        ];
        for v in vals {
            let s = v.to_string();
            let parsed: TreeHealth = s.parse().unwrap();
            assert_eq!(parsed, v);
        }
    }

    #[test]
    fn test_tree_constructor() {
        let site_id = Uuid::new_v4();
        let tree = Tree::new(site_id, "Quercus rubra".to_string());
        assert_eq!(tree.site_id, site_id);
        assert_eq!(tree.species, "Quercus rubra");
        assert_eq!(tree.health, TreeHealth::Unknown);
    }

    #[test]
    fn test_canopy_area() {
        let site_id = Uuid::new_v4();
        let mut tree = Tree::new(site_id, "Pecan".to_string());
        assert!(tree.canopy_area_sqft().is_none());

        tree.canopy_radius_ft = Some(15.0);
        let area = tree.canopy_area_sqft().unwrap();
        assert!((area - 706.86).abs() < 0.1);
    }
}
