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

/// One recent ontology event displayed in the Pulse events stream.
#[derive(Debug, Clone, Serialize)]
pub struct EventListEntry {
    pub time: String,
    pub kind: String,
    pub summary: String,
    pub severity: Option<String>,
    /// Link to the entity page of the first subject, when present.
    pub href: Option<String>,
}

/// An inline action card rendered on the Pulse page. Each card posts to
/// `/e/site/{site_id}/actions/{name}` and targets an htmx result slot.
#[derive(Debug, Clone, Serialize)]
pub struct PulseActionCard {
    pub name: String,
    pub label: String,
    pub description: String,
    pub site_id: String,
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
    pub recent_events: Vec<EventListEntry>,
    pub action_cards: Vec<PulseActionCard>,
    /// `/e/site/<id>` when a site exists; used by the Site overview link.
    pub site_href: Option<String>,
}

#[derive(Template, WebTemplate)]
#[template(path = "pages/bills.html")]
pub struct BillsPage {
    pub active_page: String,
    pub site_name: String,
    pub bills: Vec<BillSummary>,
    pub bills_chart: String,
}

// ---------------------------------------------------------------------------
// Partial templates — htmx fragment responses
// ---------------------------------------------------------------------------

#[derive(Template, WebTemplate)]
#[template(path = "partials/chart.html")]
pub struct ChartPartial {
    pub id: String,
    pub config_json: String,
    pub height: String,
}

// ---------------------------------------------------------------------------
// Universal entity page (W1)
// ---------------------------------------------------------------------------

/// A timeline event for the entity page.
#[derive(Debug, Clone, Serialize)]
pub struct TimelineEvent {
    pub time: String,
    pub kind: String,
    pub summary: String,
    pub severity: Option<String>,
}

/// A row describing a single property key/value pair, pretty-printed.
#[derive(Debug, Clone, Serialize)]
pub struct PropertyRow {
    pub key: String,
    pub value: String,
    /// `true` when `value` is a formatted nested JSON block.
    pub nested: bool,
}

#[derive(Template, WebTemplate)]
#[template(path = "pages/entity.html")]
pub struct EntityPage {
    pub active_page: String,
    pub site_name: String,
    pub kind: String,
    pub id: String,
    pub display_name: String,
    pub properties: Vec<PropertyRow>,
    pub applicable_actions: Vec<String>,
}

#[derive(Template, WebTemplate)]
#[template(path = "partials/entity_timeline.html")]
pub struct EntityTimelinePartial {
    pub events: Vec<TimelineEvent>,
}

// ---------------------------------------------------------------------------
// Property map page (W4)
// ---------------------------------------------------------------------------

#[derive(Template, WebTemplate)]
#[template(path = "pages/map.html")]
pub struct MapPage {
    pub active_page: String,
    pub site_name: String,
    /// Pre-serialized GeoJSON FeatureCollection string; embedded into the
    /// page inline as a JS literal.
    pub geojson: String,
}
