use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::temporal::BillingPeriod;
use crate::units::Usd;

/// A utility bill statement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bill {
    pub id: Uuid,
    pub account_id: Uuid,
    pub period: BillingPeriod,
    pub statement_date: NaiveDate,
    pub due_date: Option<NaiveDate>,
    pub total_usage: f64,
    pub usage_unit: String,
    pub total_amount: Usd,
    pub line_items: Vec<BillLineItem>,
    pub source_file: Option<String>,
    pub notes: Option<String>,
    pub parse_method: Option<String>,
    pub llm_model: Option<String>,
    pub llm_confidence: Option<f64>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Bill {
    pub fn new(
        account_id: Uuid,
        period_start: NaiveDate,
        period_end: NaiveDate,
        statement_date: NaiveDate,
        total_usage: f64,
        usage_unit: String,
        total_amount: Usd,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            account_id,
            period: BillingPeriod::new(period_start, period_end),
            statement_date,
            due_date: None,
            total_usage,
            usage_unit,
            total_amount,
            line_items: Vec::new(),
            source_file: None,
            notes: None,
            parse_method: None,
            llm_model: None,
            llm_confidence: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// Validate that line items sum to the total (within tolerance).
    pub fn validate_line_items(&self) -> LineItemValidation {
        let line_total: f64 = self.line_items.iter().map(|li| li.amount.value()).sum();
        let diff = (self.total_amount.value() - line_total).abs();
        if diff < 0.02 {
            LineItemValidation::Valid
        } else {
            LineItemValidation::Mismatch {
                expected: self.total_amount,
                actual: Usd::new(line_total),
                difference: Usd::new(diff),
            }
        }
    }

    /// Cost per unit of usage.
    pub fn effective_rate(&self) -> Option<Usd> {
        if self.total_usage > 0.0 {
            Some(Usd::new(self.total_amount.value() / self.total_usage))
        } else {
            None
        }
    }

    /// Average daily usage.
    pub fn daily_usage(&self) -> Option<f64> {
        let days = self.period.days();
        if days > 0 {
            Some(self.total_usage / days as f64)
        } else {
            None
        }
    }

    /// Average daily cost.
    pub fn daily_cost(&self) -> Option<Usd> {
        let days = self.period.days();
        if days > 0 {
            Some(self.total_amount / days as f64)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone)]
pub enum LineItemValidation {
    Valid,
    Mismatch {
        expected: Usd,
        actual: Usd,
        difference: Usd,
    },
}

/// A line item on a bill.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BillLineItem {
    pub id: Uuid,
    pub bill_id: Uuid,
    pub description: String,
    pub category: LineItemCategory,
    pub amount: Usd,
    pub usage: Option<f64>,
    pub rate: Option<f64>,
}

impl BillLineItem {
    pub fn new(bill_id: Uuid, description: String, category: LineItemCategory, amount: Usd) -> Self {
        Self {
            id: Uuid::new_v4(),
            bill_id,
            description,
            category,
            amount,
            usage: None,
            rate: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LineItemCategory {
    BaseCharge,
    EnergyCharge,
    DeliveryCharge,
    FuelCostAdjustment,
    DemandCharge,
    RiderCharge,
    Tax,
    Fee,
    Credit,
    Other,
}

impl std::fmt::Display for LineItemCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BaseCharge => write!(f, "Base Charge"),
            Self::EnergyCharge => write!(f, "Energy Charge"),
            Self::DeliveryCharge => write!(f, "Delivery Charge"),
            Self::FuelCostAdjustment => write!(f, "Fuel Cost Adjustment"),
            Self::DemandCharge => write!(f, "Demand Charge"),
            Self::RiderCharge => write!(f, "Rider Charge"),
            Self::Tax => write!(f, "Tax"),
            Self::Fee => write!(f, "Fee"),
            Self::Credit => write!(f, "Credit"),
            Self::Other => write!(f, "Other"),
        }
    }
}
