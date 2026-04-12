use askama::Template;
use askama_web::WebTemplate;
use serde::Serialize;

// ---------------------------------------------------------------------------
// Shared types used across multiple templates
// ---------------------------------------------------------------------------

/// A single stat card on the Pulse page.
#[derive(Debug, Clone, Serialize)]
pub struct StatCard {
    pub label: &'static str,
    pub value: String,
    pub unit: &'static str,
    pub trend: Option<f64>,
    pub color: &'static str,
    pub href: &'static str,
}

/// A single alert for the top alert bar.
#[derive(Debug, Clone, Serialize)]
pub struct Alert {
    pub message: String,
    pub severity: AlertSeverity,
    pub href: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub enum AlertSeverity {
    Warning,
    Danger,
    Info,
}

/// Experiment summary for cards.
#[derive(Debug, Clone, Serialize)]
pub struct ExperimentSummary {
    pub title: String,
    pub status: String,
    pub days_active: i64,
}

/// Recommendation summary for cards.
#[derive(Debug, Clone, Serialize)]
pub struct RecommendationSummary {
    pub title: String,
    pub category: String,
    pub annual_savings: f64,
    pub payback_years: f64,
    pub confidence: f64,
    pub description: String,
}

/// Chart.js configuration passed to templates via JSON data attribute.
#[derive(Debug, Clone, Serialize)]
pub struct ChartConfig {
    #[serde(rename = "type")]
    pub chart_type: String,
    pub data: serde_json::Value,
    pub options: serde_json::Value,
}

/// Bill summary for the bills page.
#[derive(Debug, Clone, Serialize)]
pub struct BillSummary {
    pub id: String,
    pub account_name: String,
    pub utility_type: String,
    pub period: String,
    pub usage: f64,
    pub usage_unit: String,
    pub amount: f64,
    pub daily_rate: f64,
}

/// Device summary for energy page.
#[derive(Debug, Clone, Serialize)]
pub struct DeviceSummary {
    pub name: String,
    pub kind: String,
    pub est_monthly_kwh: f64,
}

/// Circuit summary for energy page.
#[derive(Debug, Clone, Serialize)]
pub struct CircuitSummary {
    pub label: String,
    pub kwh_today: f64,
    pub pct_of_total: f64,
}

/// Pool status for water page.
#[derive(Debug, Clone, Serialize)]
pub struct PoolDisplay {
    pub name: String,
    pub volume_gallons: f64,
    pub pump_runtime_hours: Option<f64>,
    pub last_chlorine_ppm: Option<f64>,
    pub last_ph: Option<f64>,
    pub last_temp_f: Option<f64>,
}

/// Septic status for water page.
#[derive(Debug, Clone, Serialize)]
pub struct SepticDisplay {
    pub tank_capacity_gallons: f64,
    pub days_until_pump: i64,
    pub is_overdue: bool,
    pub daily_load_estimate: Option<f64>,
}

/// Property zone for the map.
#[derive(Debug, Clone, Serialize)]
pub struct ZoneDisplay {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub area_sqft: Option<f64>,
}

/// Flock summary for land page.
#[derive(Debug, Clone, Serialize)]
pub struct FlockDisplay {
    pub name: String,
    pub breed: String,
    pub bird_count: i32,
    pub eggs_today: f64,
    pub feed_today_lbs: f64,
    pub status: String,
}

/// Garden bed summary for land page.
#[derive(Debug, Clone, Serialize)]
pub struct GardenBedDisplay {
    pub name: String,
    pub bed_type: String,
    pub area_sqft: Option<f64>,
    pub active_plantings: i32,
}

/// Simulation result for the lab page.
#[derive(Debug, Clone, Serialize)]
pub struct SimulationResult {
    pub scenario: String,
    pub current_annual_cost: f64,
    pub projected_annual_cost: f64,
    pub annual_savings: f64,
    pub payback_years: Option<f64>,
}

// ---------------------------------------------------------------------------
// Page templates — each corresponds to a full-page HTML render
// ---------------------------------------------------------------------------

#[derive(Template, WebTemplate)]
#[template(path = "pages/pulse.html")]
pub struct PulsePage {
    pub active_page: String,
    pub site_name: String,
    pub briefing: Option<String>,
    pub briefing_date: String,
    pub stats: Vec<StatCard>,
    pub alerts: Vec<Alert>,
    pub experiments: Vec<ExperimentSummary>,
    pub top_recommendation: Option<RecommendationSummary>,
}

#[derive(Template, WebTemplate)]
#[template(path = "pages/energy.html")]
pub struct EnergyPage {
    pub active_page: String,
    pub site_name: String,
    pub total_kwh_today: f64,
    pub estimated_cost_today: f64,
    pub circuits: Vec<CircuitSummary>,
    pub usage_chart: String,
    pub circuit_chart: String,
    pub baseline_r_squared: Option<f64>,
    pub baseline_slope: Option<f64>,
    pub baseline_intercept: Option<f64>,
}

#[derive(Template, WebTemplate)]
#[template(path = "pages/water.html")]
pub struct WaterPage {
    pub active_page: String,
    pub site_name: String,
    pub pools: Vec<PoolDisplay>,
    pub septic: Option<SepticDisplay>,
    pub has_water_data: bool,
}

#[derive(Template, WebTemplate)]
#[template(path = "pages/property.html")]
pub struct PropertyPage {
    pub active_page: String,
    pub site_name: String,
    pub zones: Vec<ZoneDisplay>,
}

#[derive(Template, WebTemplate)]
#[template(path = "pages/land.html")]
pub struct LandPage {
    pub active_page: String,
    pub site_name: String,
    pub flocks: Vec<FlockDisplay>,
    pub garden_beds: Vec<GardenBedDisplay>,
    pub has_livestock: bool,
    pub has_garden: bool,
}

#[derive(Template, WebTemplate)]
#[template(path = "pages/lab.html")]
pub struct LabPage {
    pub active_page: String,
    pub site_name: String,
    pub recommendations: Vec<RecommendationSummary>,
    pub experiments: Vec<ExperimentSummary>,
}

#[derive(Template, WebTemplate)]
#[template(path = "pages/bills.html")]
pub struct BillsPage {
    pub active_page: String,
    pub site_name: String,
    pub bills: Vec<BillSummary>,
    pub bills_chart: String,
}

#[derive(Template, WebTemplate)]
#[template(path = "pages/chat.html")]
pub struct ChatPage {
    pub active_page: String,
    pub site_name: String,
}

// ---------------------------------------------------------------------------
// Partial templates — htmx fragment responses
// ---------------------------------------------------------------------------

#[derive(Template, WebTemplate)]
#[template(path = "partials/stat_card.html")]
pub struct StatCardPartial {
    pub card: StatCard,
}

#[derive(Template, WebTemplate)]
#[template(path = "partials/chart.html")]
pub struct ChartPartial {
    pub id: String,
    pub config_json: String,
    pub height: String,
}

#[derive(Template, WebTemplate)]
#[template(path = "partials/empty_state.html")]
pub struct EmptyStatePartial {
    pub icon: &'static str,
    pub title: &'static str,
    pub message: &'static str,
}
