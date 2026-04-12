use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::units::Usd;

/// A utility service account (electric, gas, water, trash).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UtilityAccount {
    pub id: Uuid,
    pub site_id: Uuid,
    pub provider_name: String,
    pub utility_type: UtilityType,
    pub account_number: Option<String>,
    pub meter_id: Option<String>,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl UtilityAccount {
    pub fn new(site_id: Uuid, provider_name: String, utility_type: UtilityType) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            site_id,
            provider_name,
            utility_type,
            account_number: None,
            meter_id: None,
            is_active: true,
            created_at: now,
            updated_at: now,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UtilityType {
    Electric,
    Gas,
    Water,
    Sewer,
    Trash,
    Internet,
    Propane,
}

impl std::fmt::Display for UtilityType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Electric => write!(f, "electric"),
            Self::Gas => write!(f, "gas"),
            Self::Water => write!(f, "water"),
            Self::Sewer => write!(f, "sewer"),
            Self::Trash => write!(f, "trash"),
            Self::Internet => write!(f, "internet"),
            Self::Propane => write!(f, "propane"),
        }
    }
}

impl std::str::FromStr for UtilityType {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "electric" | "electricity" => Ok(Self::Electric),
            "gas" | "natural_gas" => Ok(Self::Gas),
            "water" => Ok(Self::Water),
            "sewer" => Ok(Self::Sewer),
            "trash" | "waste" => Ok(Self::Trash),
            "internet" => Ok(Self::Internet),
            "propane" => Ok(Self::Propane),
            other => Err(format!("unknown utility type: {other}")),
        }
    }
}

/// A rate schedule / tariff structure for a utility account.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateSchedule {
    pub id: Uuid,
    pub account_id: Uuid,
    pub name: String,
    pub rate_type: RateType,
    pub effective_from: NaiveDate,
    pub effective_until: Option<NaiveDate>,
    pub base_charge: Usd,
    pub tiers: Vec<RateTier>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl RateSchedule {
    pub fn new(account_id: Uuid, name: String, rate_type: RateType, effective_from: NaiveDate) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            account_id,
            name,
            rate_type,
            effective_from,
            effective_until: None,
            base_charge: Usd::zero(),
            tiers: Vec::new(),
            created_at: now,
            updated_at: now,
        }
    }

    /// Compute cost for a given usage amount.
    pub fn compute_cost(&self, usage: f64) -> Usd {
        let mut total = self.base_charge;
        match self.rate_type {
            RateType::Flat => {
                if let Some(tier) = self.tiers.first() {
                    total = total + Usd::new(usage * tier.rate_per_unit.value());
                }
            }
            RateType::Tiered => {
                let mut remaining = usage;
                for tier in &self.tiers {
                    let tier_max = tier.upper_limit.unwrap_or(f64::MAX);
                    let tier_min = tier.lower_limit;
                    let tier_usage = remaining.min(tier_max - tier_min);
                    if tier_usage > 0.0 {
                        total = total + Usd::new(tier_usage * tier.rate_per_unit.value());
                        remaining -= tier_usage;
                    }
                    if remaining <= 0.0 {
                        break;
                    }
                }
            }
            RateType::TimeOfUse | RateType::Demand => {
                // TOU and demand require time-tagged readings; flat approximation here
                if let Some(tier) = self.tiers.first() {
                    total = total + Usd::new(usage * tier.rate_per_unit.value());
                }
            }
        }
        total
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RateType {
    Flat,
    Tiered,
    TimeOfUse,
    Demand,
}

impl std::fmt::Display for RateType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Flat => write!(f, "flat"),
            Self::Tiered => write!(f, "tiered"),
            Self::TimeOfUse => write!(f, "time-of-use"),
            Self::Demand => write!(f, "demand"),
        }
    }
}

/// A tier within a rate schedule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateTier {
    pub label: String,
    pub lower_limit: f64,
    pub upper_limit: Option<f64>,
    pub rate_per_unit: Usd,
    /// For TOU schedules: which hours this tier applies to.
    pub peak_hours: Option<String>,
}
