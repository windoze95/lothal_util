use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::units::{Acres, SquareFeet};

/// The top-level property entity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Site {
    pub id: Uuid,
    pub address: String,
    pub city: String,
    pub state: String,
    pub zip: String,
    pub latitude: f64,
    pub longitude: f64,
    pub lot_size: Acres,
    pub climate_zone: Option<String>,
    pub soil_type: Option<SoilType>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Site {
    pub fn new(address: String, city: String, state: String, zip: String) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            address,
            city,
            state,
            zip,
            latitude: 0.0,
            longitude: 0.0,
            lot_size: Acres::zero(),
            climate_zone: None,
            soil_type: None,
            created_at: now,
            updated_at: now,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SoilType {
    Clay,
    Loam,
    Sand,
    Silt,
    Unknown,
}

impl std::fmt::Display for SoilType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Clay => write!(f, "clay"),
            Self::Loam => write!(f, "loam"),
            Self::Sand => write!(f, "sand"),
            Self::Silt => write!(f, "silt"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

impl std::str::FromStr for SoilType {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "clay" => Ok(Self::Clay),
            "loam" => Ok(Self::Loam),
            "sand" => Ok(Self::Sand),
            "silt" => Ok(Self::Silt),
            "unknown" => Ok(Self::Unknown),
            other => Err(format!("unknown soil type: {other}")),
        }
    }
}

/// A building on the site.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Structure {
    pub id: Uuid,
    pub site_id: Uuid,
    pub name: String,
    pub year_built: Option<i32>,
    pub square_footage: SquareFeet,
    pub stories: Option<i32>,
    pub foundation_type: Option<FoundationType>,
    pub has_pool: bool,
    pub pool_gallons: Option<f64>,
    pub has_septic: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Structure {
    pub fn new(site_id: Uuid, name: String) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            site_id,
            name,
            year_built: None,
            square_footage: SquareFeet::zero(),
            stories: None,
            foundation_type: None,
            has_pool: false,
            pool_gallons: None,
            has_septic: false,
            created_at: now,
            updated_at: now,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FoundationType {
    Slab,
    Crawlspace,
    Basement,
    Pier,
    Unknown,
}

impl std::fmt::Display for FoundationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Slab => write!(f, "slab"),
            Self::Crawlspace => write!(f, "crawlspace"),
            Self::Basement => write!(f, "basement"),
            Self::Pier => write!(f, "pier"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

impl std::str::FromStr for FoundationType {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "slab" => Ok(Self::Slab),
            "crawlspace" | "crawl" => Ok(Self::Crawlspace),
            "basement" => Ok(Self::Basement),
            "pier" => Ok(Self::Pier),
            "unknown" => Ok(Self::Unknown),
            other => Err(format!("unknown foundation type: {other}")),
        }
    }
}

/// A room or HVAC zone within a structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Zone {
    pub id: Uuid,
    pub structure_id: Uuid,
    pub name: String,
    pub floor: Option<i32>,
    pub square_footage: Option<SquareFeet>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Zone {
    pub fn new(structure_id: Uuid, name: String) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            structure_id,
            name,
            floor: None,
            square_footage: None,
            created_at: now,
            updated_at: now,
        }
    }
}
