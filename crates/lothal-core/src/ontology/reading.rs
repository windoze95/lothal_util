use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A time-series measurement from a sensor or meter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reading {
    pub time: DateTime<Utc>,
    pub source: ReadingSource,
    pub kind: ReadingKind,
    pub value: f64,
    pub metadata: Option<serde_json::Value>,
}

impl Reading {
    pub fn new(source: ReadingSource, kind: ReadingKind, value: f64) -> Self {
        Self {
            time: Utc::now(),
            source,
            kind,
            value,
            metadata: None,
        }
    }

    pub fn at(time: DateTime<Utc>, source: ReadingSource, kind: ReadingKind, value: f64) -> Self {
        Self {
            time,
            source,
            kind,
            value,
            metadata: None,
        }
    }
}

/// Where a reading came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "id")]
pub enum ReadingSource {
    Device(Uuid),
    Circuit(Uuid),
    Zone(Uuid),
    Meter(Uuid),
}

impl ReadingSource {
    pub fn source_type(&self) -> &'static str {
        match self {
            Self::Device(_) => "device",
            Self::Circuit(_) => "circuit",
            Self::Zone(_) => "zone",
            Self::Meter(_) => "meter",
        }
    }

    pub fn source_id(&self) -> Uuid {
        match self {
            Self::Device(id) | Self::Circuit(id) | Self::Zone(id) | Self::Meter(id) => *id,
        }
    }
}

/// What kind of measurement this reading represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadingKind {
    /// Cumulative energy in kWh
    ElectricKwh,
    /// Instantaneous power in watts
    ElectricWatts,
    /// Gas consumption in therms
    GasTherms,
    /// Water consumption in gallons
    WaterGallons,
    /// Temperature in Fahrenheit
    TemperatureF,
    /// Relative humidity percentage
    HumidityPct,
    /// HVAC runtime in minutes
    RuntimeMinutes,
    /// Solar irradiance in W/m²
    SolarIrradiance,
    /// Water flow rate in GPM
    WaterFlowGpm,
}

impl std::fmt::Display for ReadingKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ElectricKwh => write!(f, "kWh"),
            Self::ElectricWatts => write!(f, "watts"),
            Self::GasTherms => write!(f, "therms"),
            Self::WaterGallons => write!(f, "gallons"),
            Self::TemperatureF => write!(f, "°F"),
            Self::HumidityPct => write!(f, "%RH"),
            Self::RuntimeMinutes => write!(f, "min"),
            Self::SolarIrradiance => write!(f, "W/m²"),
            Self::WaterFlowGpm => write!(f, "GPM"),
        }
    }
}

impl ReadingKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ElectricKwh => "electric_kwh",
            Self::ElectricWatts => "electric_watts",
            Self::GasTherms => "gas_therms",
            Self::WaterGallons => "water_gallons",
            Self::TemperatureF => "temperature_f",
            Self::HumidityPct => "humidity_pct",
            Self::RuntimeMinutes => "runtime_minutes",
            Self::SolarIrradiance => "solar_irradiance",
            Self::WaterFlowGpm => "water_flow_gpm",
        }
    }
}

impl std::str::FromStr for ReadingKind {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "electric_kwh" => Ok(Self::ElectricKwh),
            "electric_watts" => Ok(Self::ElectricWatts),
            "gas_therms" => Ok(Self::GasTherms),
            "water_gallons" => Ok(Self::WaterGallons),
            "temperature_f" => Ok(Self::TemperatureF),
            "humidity_pct" => Ok(Self::HumidityPct),
            "runtime_minutes" => Ok(Self::RuntimeMinutes),
            "solar_irradiance" => Ok(Self::SolarIrradiance),
            "water_flow_gpm" => Ok(Self::WaterFlowGpm),
            other => Err(format!("unknown reading kind: {other}")),
        }
    }
}
