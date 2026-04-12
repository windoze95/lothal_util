use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::units::{Usd, Watts};

/// A device that consumes resources (electricity, gas, water).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Device {
    pub id: Uuid,
    pub structure_id: Uuid,
    pub zone_id: Option<Uuid>,
    pub circuit_id: Option<Uuid>,
    pub name: String,
    pub kind: DeviceKind,
    pub make: Option<String>,
    pub model: Option<String>,
    pub nameplate_watts: Option<Watts>,
    pub estimated_daily_hours: Option<f64>,
    pub year_installed: Option<i32>,
    pub expected_lifespan_years: Option<i32>,
    pub replacement_cost: Option<Usd>,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Device {
    pub fn new(structure_id: Uuid, name: String, kind: DeviceKind) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            structure_id,
            zone_id: None,
            circuit_id: None,
            name,
            kind,
            make: None,
            model: None,
            nameplate_watts: None,
            estimated_daily_hours: None,
            year_installed: None,
            expected_lifespan_years: None,
            replacement_cost: None,
            notes: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// Estimate annual kWh based on nameplate watts and daily run hours.
    pub fn estimated_annual_kwh(&self) -> Option<f64> {
        let watts = self.nameplate_watts?.value();
        let hours = self.estimated_daily_hours?;
        Some(watts * hours * 365.0 / 1000.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceKind {
    // HVAC
    AirConditioner,
    Furnace,
    HeatPump,
    AirHandler,
    Thermostat,
    // Water
    WaterHeater,
    WaterSoftener,
    WellPump,
    // Pool
    PoolPump,
    PoolHeater,
    PoolCleaner,
    // Kitchen
    Refrigerator,
    Freezer,
    Dishwasher,
    Oven,
    Range,
    Microwave,
    // Laundry
    Washer,
    Dryer,
    // Comfort
    Dehumidifier,
    Humidifier,
    CeilingFan,
    SpaceHeater,
    // Infrastructure
    ElectricalPanel,
    SumpPump,
    GarageDoor,
    SecuritySystem,
    // Tech
    Server,
    NetworkSwitch,
    UPS,
    // Outdoor
    Sprinkler,
    OutdoorLighting,
    EvCharger,
    // Catch-all
    Other,
}

impl std::fmt::Display for DeviceKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = serde_json::to_string(self).unwrap_or_else(|_| "unknown".into());
        // Strip the quotes from JSON serialization
        write!(f, "{}", s.trim_matches('"'))
    }
}

impl std::str::FromStr for DeviceKind {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let quoted = format!("\"{s}\"");
        serde_json::from_str(&quoted).map_err(|_| format!("unknown device kind: {s}"))
    }
}
