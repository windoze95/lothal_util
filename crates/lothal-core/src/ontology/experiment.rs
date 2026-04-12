use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::temporal::DateRange;
use crate::units::Usd;

/// A testable efficiency hypothesis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hypothesis {
    pub id: Uuid,
    pub site_id: Uuid,
    pub title: String,
    pub description: String,
    pub expected_savings_pct: Option<f64>,
    pub expected_savings_usd: Option<Usd>,
    pub category: HypothesisCategory,
    pub created_at: DateTime<Utc>,
}

impl Hypothesis {
    pub fn new(site_id: Uuid, title: String, description: String, category: HypothesisCategory) -> Self {
        Self {
            id: Uuid::new_v4(),
            site_id,
            title,
            description,
            expected_savings_pct: None,
            expected_savings_usd: None,
            category,
            created_at: Utc::now(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HypothesisCategory {
    DeviceSwap,
    BehaviorChange,
    EnvelopeUpgrade,
    RateOptimization,
    LoadShifting,
    Maintenance,
    WaterConservation,
    LivestockOptimization,
    LandManagement,
    Other,
}

impl std::fmt::Display for HypothesisCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DeviceSwap => write!(f, "Device Swap"),
            Self::BehaviorChange => write!(f, "Behavior Change"),
            Self::EnvelopeUpgrade => write!(f, "Envelope Upgrade"),
            Self::RateOptimization => write!(f, "Rate Optimization"),
            Self::LoadShifting => write!(f, "Load Shifting"),
            Self::Maintenance => write!(f, "Maintenance"),
            Self::WaterConservation => write!(f, "Water Conservation"),
            Self::LivestockOptimization => write!(f, "Livestock Optimization"),
            Self::LandManagement => write!(f, "Land Management"),
            Self::Other => write!(f, "Other"),
        }
    }
}

/// An actual change made to test a hypothesis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Intervention {
    pub id: Uuid,
    pub site_id: Uuid,
    pub device_id: Option<Uuid>,
    pub description: String,
    pub date_applied: NaiveDate,
    pub cost: Option<Usd>,
    pub reversible: bool,
    pub created_at: DateTime<Utc>,
}

impl Intervention {
    pub fn new(site_id: Uuid, description: String, date_applied: NaiveDate) -> Self {
        Self {
            id: Uuid::new_v4(),
            site_id,
            device_id: None,
            description,
            date_applied,
            cost: None,
            reversible: true,
            created_at: Utc::now(),
        }
    }
}

/// An experiment linking a hypothesis, intervention, and measured outcome.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Experiment {
    pub id: Uuid,
    pub site_id: Uuid,
    pub hypothesis_id: Uuid,
    pub intervention_id: Uuid,
    pub baseline_period: DateRange,
    pub result_period: DateRange,
    pub status: ExperimentStatus,
    pub actual_savings_pct: Option<f64>,
    pub actual_savings_usd: Option<Usd>,
    pub confidence: Option<f64>,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Experiment {
    pub fn new(
        site_id: Uuid,
        hypothesis_id: Uuid,
        intervention_id: Uuid,
        baseline_period: DateRange,
        result_period: DateRange,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            site_id,
            hypothesis_id,
            intervention_id,
            baseline_period,
            result_period,
            status: ExperimentStatus::Active,
            actual_savings_pct: None,
            actual_savings_usd: None,
            confidence: None,
            notes: None,
            created_at: now,
            updated_at: now,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExperimentStatus {
    Planned,
    Active,
    Completed,
    Inconclusive,
    Cancelled,
}

impl std::fmt::Display for ExperimentStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Planned => write!(f, "Planned"),
            Self::Active => write!(f, "Active"),
            Self::Completed => write!(f, "Completed"),
            Self::Inconclusive => write!(f, "Inconclusive"),
            Self::Cancelled => write!(f, "Cancelled"),
        }
    }
}

/// A generated efficiency recommendation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recommendation {
    pub id: Uuid,
    pub site_id: Uuid,
    pub device_id: Option<Uuid>,
    pub title: String,
    pub description: String,
    pub category: HypothesisCategory,
    pub estimated_annual_savings: Usd,
    pub estimated_capex: Usd,
    pub payback_years: f64,
    pub confidence: f64,
    pub priority_score: f64,
    pub data_requirements: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl Recommendation {
    pub fn new(
        site_id: Uuid,
        title: String,
        description: String,
        category: HypothesisCategory,
        annual_savings: Usd,
        capex: Usd,
    ) -> Self {
        let payback = if annual_savings.value() > 0.0 {
            capex.value() / annual_savings.value()
        } else {
            f64::INFINITY
        };
        // Priority: higher savings / lower payback = higher priority
        let priority = if payback > 0.0 && payback.is_finite() {
            annual_savings.value() / payback
        } else {
            0.0
        };
        Self {
            id: Uuid::new_v4(),
            site_id,
            device_id: None,
            title,
            description,
            category,
            estimated_annual_savings: annual_savings,
            estimated_capex: capex,
            payback_years: payback,
            confidence: 0.5, // default medium confidence
            priority_score: priority,
            data_requirements: None,
            created_at: Utc::now(),
        }
    }
}
