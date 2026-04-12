use serde_json::{json, Value};

use crate::templates::ChartConfig;

// Color constants matching our dark theme palette.
const ENERGY_COLOR: &str = "#f7c948";
const WATER_COLOR: &str = "#5bc0eb";
const BIO_COLOR: &str = "#8ac926";
const HEAT_COLOR: &str = "#f76c6c";
const POSITIVE_COLOR: &str = "#3dd68c";
const NEUTRAL_COLOR: &str = "#4f9cf7";
const MUTED_COLOR: &str = "#555a6e";
const GRID_COLOR: &str = "rgba(46, 51, 70, 0.6)";
const TEXT_COLOR: &str = "#8b8fa3";

/// Shared Chart.js options for the dark theme.
fn dark_theme_options() -> Value {
    json!({
        "responsive": true,
        "maintainAspectRatio": false,
        "plugins": {
            "legend": {
                "labels": { "color": TEXT_COLOR, "font": { "family": "system-ui" } }
            },
            "tooltip": {
                "backgroundColor": "#232736",
                "titleColor": "#e8eaed",
                "bodyColor": "#8b8fa3",
                "borderColor": "#2e3346",
                "borderWidth": 1
            }
        },
        "scales": {
            "x": {
                "ticks": { "color": TEXT_COLOR },
                "grid": { "color": GRID_COLOR }
            },
            "y": {
                "ticks": { "color": TEXT_COLOR },
                "grid": { "color": GRID_COLOR }
            }
        }
    })
}

/// Build a line chart config for energy usage over time.
pub fn energy_usage_chart(labels: Vec<String>, actual: Vec<f64>, predicted: Vec<f64>) -> ChartConfig {
    let mut opts = dark_theme_options();
    opts["plugins"]["legend"]["display"] = json!(true);
    opts["scales"]["y"]["title"] = json!({"display": true, "text": "kWh", "color": TEXT_COLOR});

    ChartConfig {
        chart_type: "line".into(),
        data: json!({
            "labels": labels,
            "datasets": [
                {
                    "label": "Actual",
                    "data": actual,
                    "borderColor": ENERGY_COLOR,
                    "backgroundColor": format!("{ENERGY_COLOR}33"),
                    "fill": true,
                    "tension": 0.3,
                    "pointRadius": 2
                },
                {
                    "label": "Baseline",
                    "data": predicted,
                    "borderColor": MUTED_COLOR,
                    "borderDash": [5, 5],
                    "fill": false,
                    "tension": 0.3,
                    "pointRadius": 0
                }
            ]
        }),
        options: opts,
    }
}

/// Build a doughnut chart for circuit breakdown.
pub fn circuit_breakdown_chart(labels: Vec<String>, values: Vec<f64>) -> ChartConfig {
    let colors = vec![
        ENERGY_COLOR, WATER_COLOR, HEAT_COLOR, BIO_COLOR,
        NEUTRAL_COLOR, POSITIVE_COLOR, "#c084fc", "#fb923c",
    ];
    let bg_colors: Vec<&str> = colors.iter().cycle().take(labels.len()).copied().collect();

    let mut opts = dark_theme_options();
    // Remove axis scales for doughnut
    opts.as_object_mut().unwrap().remove("scales");
    opts["plugins"]["legend"]["position"] = json!("right");
    opts["cutout"] = json!("60%");

    ChartConfig {
        chart_type: "doughnut".into(),
        data: json!({
            "labels": labels,
            "datasets": [{
                "data": values,
                "backgroundColor": bg_colors,
                "borderColor": "#1a1d27",
                "borderWidth": 2
            }]
        }),
        options: opts,
    }
}

/// Build a stacked bar chart for monthly bills by utility type.
pub fn bills_stacked_chart(
    months: Vec<String>,
    electric: Vec<f64>,
    gas: Vec<f64>,
    water: Vec<f64>,
) -> ChartConfig {
    let mut opts = dark_theme_options();
    opts["scales"]["x"]["stacked"] = json!(true);
    opts["scales"]["y"]["stacked"] = json!(true);
    opts["scales"]["y"]["title"] = json!({"display": true, "text": "$", "color": TEXT_COLOR});

    ChartConfig {
        chart_type: "bar".into(),
        data: json!({
            "labels": months,
            "datasets": [
                {
                    "label": "Electric",
                    "data": electric,
                    "backgroundColor": ENERGY_COLOR,
                    "borderRadius": 2
                },
                {
                    "label": "Gas",
                    "data": gas,
                    "backgroundColor": HEAT_COLOR,
                    "borderRadius": 2
                },
                {
                    "label": "Water",
                    "data": water,
                    "backgroundColor": WATER_COLOR,
                    "borderRadius": 2
                }
            ]
        }),
        options: opts,
    }
}

/// Build a line chart for egg production over time.
pub fn egg_production_chart(labels: Vec<String>, counts: Vec<f64>) -> ChartConfig {
    let mut opts = dark_theme_options();
    opts["scales"]["y"]["title"] = json!({"display": true, "text": "Eggs", "color": TEXT_COLOR});
    opts["plugins"]["legend"]["display"] = json!(false);

    ChartConfig {
        chart_type: "line".into(),
        data: json!({
            "labels": labels,
            "datasets": [{
                "label": "Eggs",
                "data": counts,
                "borderColor": BIO_COLOR,
                "backgroundColor": format!("{BIO_COLOR}33"),
                "fill": true,
                "tension": 0.3,
                "pointRadius": 3
            }]
        }),
        options: opts,
    }
}

/// Serialize a ChartConfig to a JSON string for embedding in a data attribute.
pub fn to_chart_json(config: &ChartConfig) -> String {
    serde_json::to_string(config).unwrap_or_default()
}
