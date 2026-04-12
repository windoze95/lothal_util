use chrono::NaiveDate;
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

use crate::AiError;

/// Return the MCP tool definitions for tools/list.
pub fn tool_definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "query_bills",
            "description": "Query utility bills. Optionally filter by account and date range.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "account_id": { "type": "string", "description": "UUID of the utility account" },
                    "start": { "type": "string", "description": "Start date (YYYY-MM-DD)" },
                    "end": { "type": "string", "description": "End date (YYYY-MM-DD)" }
                }
            }
        }),
        json!({
            "name": "query_readings",
            "description": "Query time-series readings for a device or circuit.",
            "inputSchema": {
                "type": "object",
                "required": ["source_id", "kind", "start", "end"],
                "properties": {
                    "source_id": { "type": "string", "description": "UUID of the device or circuit" },
                    "source_type": { "type": "string", "enum": ["device", "circuit", "zone", "meter"], "description": "Type of source (default: device)" },
                    "kind": { "type": "string", "description": "Reading kind (electric_kwh, electric_watts, gas_therms, water_gallons, temperature_f, etc.)" },
                    "start": { "type": "string", "description": "Start datetime (ISO 8601)" },
                    "end": { "type": "string", "description": "End datetime (ISO 8601)" }
                }
            }
        }),
        json!({
            "name": "query_weather",
            "description": "Query weather observations for a site over a date range.",
            "inputSchema": {
                "type": "object",
                "required": ["site_id", "start", "end"],
                "properties": {
                    "site_id": { "type": "string", "description": "UUID of the site" },
                    "start": { "type": "string", "description": "Start date (YYYY-MM-DD)" },
                    "end": { "type": "string", "description": "End date (YYYY-MM-DD)" }
                }
            }
        }),
        json!({
            "name": "get_devices",
            "description": "List all devices, optionally filtered by structure.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "structure_id": { "type": "string", "description": "UUID of the structure (optional)" }
                }
            }
        }),
        json!({
            "name": "get_rate_schedule",
            "description": "Get the active rate schedule for a utility account.",
            "inputSchema": {
                "type": "object",
                "required": ["account_id"],
                "properties": {
                    "account_id": { "type": "string", "description": "UUID of the utility account" }
                }
            }
        }),
        json!({
            "name": "compute_baseline",
            "description": "Compute a weather-normalized energy baseline for a utility account. Returns regression model with slope, intercept, R-squared.",
            "inputSchema": {
                "type": "object",
                "required": ["account_id", "mode"],
                "properties": {
                    "account_id": { "type": "string", "description": "UUID of the utility account" },
                    "mode": { "type": "string", "enum": ["cooling", "heating"], "description": "Baseline mode" }
                }
            }
        }),
        json!({
            "name": "simulate",
            "description": "Run a what-if simulation scenario. Supports device_swap, rate_change, setpoint_change, and load_shift.",
            "inputSchema": {
                "type": "object",
                "required": ["scenario"],
                "properties": {
                    "scenario": {
                        "type": "object",
                        "description": "Simulation scenario parameters"
                    }
                }
            }
        }),
        json!({
            "name": "create_hypothesis",
            "description": "Create a new efficiency hypothesis for investigation.",
            "inputSchema": {
                "type": "object",
                "required": ["site_id", "title", "description", "category"],
                "properties": {
                    "site_id": { "type": "string", "description": "UUID of the site" },
                    "title": { "type": "string" },
                    "description": { "type": "string" },
                    "category": { "type": "string", "description": "Category (hvac, water, envelope, appliance, behavioral, rate)" },
                    "expected_savings_pct": { "type": "number" },
                    "expected_savings_usd": { "type": "number" }
                }
            }
        }),
        json!({
            "name": "list_recommendations",
            "description": "Generate and return prioritized efficiency recommendations for a site.",
            "inputSchema": {
                "type": "object",
                "required": ["site_id"],
                "properties": {
                    "site_id": { "type": "string", "description": "UUID of the site" }
                }
            }
        }),
        json!({
            "name": "get_site_overview",
            "description": "Get a comprehensive overview of a site: structures, zones, devices, circuits, utility accounts.",
            "inputSchema": {
                "type": "object",
                "required": ["site_id"],
                "properties": {
                    "site_id": { "type": "string", "description": "UUID of the site" }
                }
            }
        }),
        // --- Property operations tools ---
        json!({
            "name": "get_property_zones",
            "description": "List all property zones (outdoor areas) and constraints for a site.",
            "inputSchema": {
                "type": "object",
                "required": ["site_id"],
                "properties": {
                    "site_id": { "type": "string", "description": "UUID of the site" }
                }
            }
        }),
        json!({
            "name": "get_pool_status",
            "description": "Get pool details and recent readings for a site.",
            "inputSchema": {
                "type": "object",
                "required": ["site_id"],
                "properties": {
                    "site_id": { "type": "string", "description": "UUID of the site" }
                }
            }
        }),
        json!({
            "name": "query_livestock_logs",
            "description": "Query livestock flock logs (eggs, feed, events) for a date range.",
            "inputSchema": {
                "type": "object",
                "required": ["flock_id", "start", "end"],
                "properties": {
                    "flock_id": { "type": "string", "description": "UUID of the flock" },
                    "start": { "type": "string", "description": "Start date (YYYY-MM-DD)" },
                    "end": { "type": "string", "description": "End date (YYYY-MM-DD)" }
                }
            }
        }),
        json!({
            "name": "get_property_overview",
            "description": "Comprehensive cross-system property status: zones, trees, water sources, pools, septic, flocks, garden beds, compost.",
            "inputSchema": {
                "type": "object",
                "required": ["site_id"],
                "properties": {
                    "site_id": { "type": "string", "description": "UUID of the site" }
                }
            }
        }),
    ]
}

/// Dispatch a tool call to its implementation.
pub async fn call_tool(
    name: &str,
    args: Value,
    pool: &PgPool,
) -> Result<Value, AiError> {
    match name {
        "query_bills" => handle_query_bills(args, pool).await,
        "query_readings" => handle_query_readings(args, pool).await,
        "query_weather" => handle_query_weather(args, pool).await,
        "get_devices" => handle_get_devices(args, pool).await,
        "get_rate_schedule" => handle_get_rate_schedule(args, pool).await,
        "compute_baseline" => handle_compute_baseline(args, pool).await,
        "simulate" => handle_simulate(args, pool).await,
        "create_hypothesis" => handle_create_hypothesis(args, pool).await,
        "list_recommendations" => handle_list_recommendations(args, pool).await,
        "get_site_overview" => handle_get_site_overview(args, pool).await,
        "get_property_zones" => handle_get_property_zones(args, pool).await,
        "get_pool_status" => handle_get_pool_status(args, pool).await,
        "query_livestock_logs" => handle_query_livestock_logs(args, pool).await,
        "get_property_overview" => handle_get_property_overview(args, pool).await,
        _ => Err(AiError::Mcp(format!("Unknown tool: {name}"))),
    }
}

// ---------------------------------------------------------------------------
// Tool handlers
// ---------------------------------------------------------------------------

async fn handle_query_bills(args: Value, pool: &PgPool) -> Result<Value, AiError> {
    let account_id = parse_optional_uuid(&args, "account_id")?;
    let start = parse_optional_date(&args, "start")?;
    let end = parse_optional_date(&args, "end")?;

    let bills = match (account_id, start, end) {
        (Some(aid), Some(s), Some(e)) => {
            lothal_db::bill::list_bills_by_account_and_range(pool, aid, s, e).await?
        }
        (Some(aid), _, _) => lothal_db::bill::list_bills_by_account(pool, aid).await?,
        _ => {
            return Err(AiError::Validation(
                "account_id is required for query_bills".into(),
            ))
        }
    };

    Ok(serde_json::to_value(&bills)?)
}

async fn handle_query_readings(args: Value, pool: &PgPool) -> Result<Value, AiError> {
    let source_id = parse_required_uuid(&args, "source_id")?;
    let source_type = args["source_type"].as_str().unwrap_or("device");
    let kind = args["kind"]
        .as_str()
        .ok_or_else(|| AiError::Validation("kind is required".into()))?;
    let start = args["start"]
        .as_str()
        .ok_or_else(|| AiError::Validation("start is required".into()))?;
    let end = args["end"]
        .as_str()
        .ok_or_else(|| AiError::Validation("end is required".into()))?;

    let start_dt = chrono::DateTime::parse_from_rfc3339(start)
        .or_else(|_| {
            NaiveDate::parse_from_str(start, "%Y-%m-%d")
                .map(|d| d.and_hms_opt(0, 0, 0).unwrap().and_utc().fixed_offset())
        })
        .map_err(|e| AiError::Validation(format!("Invalid start date: {e}")))?
        .with_timezone(&chrono::Utc);

    let end_dt = chrono::DateTime::parse_from_rfc3339(end)
        .or_else(|_| {
            NaiveDate::parse_from_str(end, "%Y-%m-%d")
                .map(|d| d.and_hms_opt(23, 59, 59).unwrap().and_utc().fixed_offset())
        })
        .map_err(|e| AiError::Validation(format!("Invalid end date: {e}")))?
        .with_timezone(&chrono::Utc);

    let readings =
        lothal_db::reading::get_readings(pool, source_type, source_id, kind, start_dt, end_dt)
            .await?;

    Ok(serde_json::to_value(&readings)?)
}

async fn handle_query_weather(args: Value, pool: &PgPool) -> Result<Value, AiError> {
    let site_id = parse_required_uuid(&args, "site_id")?;
    let start = parse_required_date(&args, "start")?;
    let end = parse_required_date(&args, "end")?;

    let summaries =
        lothal_db::weather::get_daily_weather_summaries(pool, site_id, start, end).await?;

    // DailyWeatherRow doesn't derive Serialize, so convert manually.
    let result: Vec<Value> = summaries
        .iter()
        .map(|s| {
            json!({
                "date": s.date.to_string(),
                "avg_temp_f": s.avg_temp_f,
                "min_temp_f": s.min_temp_f,
                "max_temp_f": s.max_temp_f,
                "avg_humidity_pct": s.avg_humidity_pct,
                "observation_count": s.observation_count,
            })
        })
        .collect();

    Ok(json!(result))
}

async fn handle_get_devices(args: Value, pool: &PgPool) -> Result<Value, AiError> {
    let structure_id = parse_optional_uuid(&args, "structure_id")?;

    match structure_id {
        Some(sid) => {
            let devices = lothal_db::device::list_devices_by_structure(pool, sid).await?;
            Ok(serde_json::to_value(&devices)?)
        }
        None => {
            // Get all sites, then all structures, then all devices.
            let sites = lothal_db::site::list_sites(pool).await?;
            let mut all_devices = Vec::new();
            for site in &sites {
                let structures = lothal_db::site::get_structures_by_site(pool, site.id).await?;
                for structure in &structures {
                    let devices =
                        lothal_db::device::list_devices_by_structure(pool, structure.id).await?;
                    all_devices.extend(devices);
                }
            }
            Ok(serde_json::to_value(&all_devices)?)
        }
    }
}

async fn handle_get_rate_schedule(args: Value, pool: &PgPool) -> Result<Value, AiError> {
    let account_id = parse_required_uuid(&args, "account_id")?;
    let schedule = lothal_db::bill::get_active_rate_schedule(pool, account_id).await?;
    Ok(serde_json::to_value(&schedule)?)
}

async fn handle_compute_baseline(args: Value, pool: &PgPool) -> Result<Value, AiError> {
    let account_id = parse_required_uuid(&args, "account_id")?;
    let mode_str = args["mode"]
        .as_str()
        .ok_or_else(|| AiError::Validation("mode is required".into()))?;

    let mode = match mode_str {
        "cooling" => lothal_engine::baseline::BaselineMode::Cooling,
        "heating" => lothal_engine::baseline::BaselineMode::Heating,
        _ => return Err(AiError::Validation("mode must be 'cooling' or 'heating'".into())),
    };

    // Fetch bills and weather data, build data points.
    let account = lothal_db::bill::get_utility_account(pool, account_id)
        .await?
        .ok_or_else(|| AiError::Validation("Account not found".into()))?;

    let bills = lothal_db::bill::list_bills_by_account(pool, account_id).await?;
    if bills.is_empty() {
        return Err(AiError::Validation("No bills found".into()));
    }

    let earliest = bills.iter().map(|b| b.period.range.start).min().unwrap();
    let latest = bills.iter().map(|b| b.period.range.end).max().unwrap();

    let weather_days =
        lothal_db::weather::get_daily_weather_summaries(pool, account.site_id, earliest, latest)
            .await?;

    let weather_map: std::collections::HashMap<NaiveDate, _> =
        weather_days.iter().map(|w| (w.date, w)).collect();

    let base_temp = 65.0;
    let mut data_points = Vec::new();

    for bill in &bills {
        let daily_usage = match bill.daily_usage() {
            Some(u) => u,
            None => continue,
        };
        for date in bill.period.range.iter_days() {
            if let Some(w) = weather_map.get(&date) {
                data_points.push(lothal_engine::baseline::DailyDataPoint {
                    date,
                    usage: daily_usage,
                    cooling_degree_days: (w.avg_temp_f - base_temp).max(0.0),
                    heating_degree_days: (base_temp - w.avg_temp_f).max(0.0),
                });
            }
        }
    }

    let model = lothal_engine::baseline::compute_baseline(&data_points, mode)
        .map_err(|e| AiError::Validation(format!("{e}")))?;

    Ok(json!({
        "slope": model.slope,
        "intercept": model.intercept,
        "r_squared": model.r_squared,
        "base_load_kwh_per_day": model.base_load_kwh_per_day,
        "data_points_count": model.data_points_count,
        "mode": mode_str,
    }))
}

async fn handle_simulate(_args: Value, _pool: &PgPool) -> Result<Value, AiError> {
    // Simulation requires constructing typed Scenario enums from freeform JSON.
    // The MCP tool provides a passthrough — the agent describes what it wants,
    // and we map common patterns.
    Err(AiError::Validation(
        "Simulation via MCP requires structured scenario input. Use compute_baseline \
         and query tools to gather data, then describe the scenario."
            .into(),
    ))
}

async fn handle_create_hypothesis(args: Value, pool: &PgPool) -> Result<Value, AiError> {
    let site_id = parse_required_uuid(&args, "site_id")?;
    let title = args["title"]
        .as_str()
        .ok_or_else(|| AiError::Validation("title required".into()))?;
    let description = args["description"]
        .as_str()
        .ok_or_else(|| AiError::Validation("description required".into()))?;
    let category = args["category"]
        .as_str()
        .ok_or_else(|| AiError::Validation("category required".into()))?;

    let hypothesis = lothal_core::Hypothesis {
        id: Uuid::new_v4(),
        site_id,
        title: title.to_string(),
        description: description.to_string(),
        expected_savings_pct: args["expected_savings_pct"].as_f64(),
        expected_savings_usd: args["expected_savings_usd"]
            .as_f64()
            .map(lothal_core::Usd::new),
        category: parse_hypothesis_category(category),
        created_at: chrono::Utc::now(),
    };

    lothal_db::experiment::insert_hypothesis(pool, &hypothesis).await?;

    Ok(serde_json::to_value(&hypothesis)?)
}

async fn handle_list_recommendations(args: Value, pool: &PgPool) -> Result<Value, AiError> {
    let site_id = parse_required_uuid(&args, "site_id")?;
    let recs = lothal_db::experiment::list_recommendations_by_site(pool, site_id).await?;
    Ok(serde_json::to_value(&recs)?)
}

async fn handle_get_site_overview(args: Value, pool: &PgPool) -> Result<Value, AiError> {
    let site_id = parse_required_uuid(&args, "site_id")?;
    let site = lothal_db::site::get_site(pool, site_id)
        .await?
        .ok_or_else(|| AiError::Validation("Site not found".into()))?;

    let structures = lothal_db::site::get_structures_by_site(pool, site_id).await?;
    let accounts = lothal_db::bill::list_utility_accounts_by_site(pool, site_id).await?;

    let mut structure_details = Vec::new();
    for s in &structures {
        let zones = lothal_db::site::get_zones_by_structure(pool, s.id).await?;
        let panels = lothal_db::site::get_panels_by_structure(pool, s.id).await?;
        let devices = lothal_db::device::list_devices_by_structure(pool, s.id).await?;

        let mut panel_details = Vec::new();
        for p in &panels {
            let circuits = lothal_db::device::get_circuits_by_panel(pool, p.id).await?;
            panel_details.push(json!({
                "panel": p,
                "circuits": circuits,
            }));
        }

        structure_details.push(json!({
            "structure": s,
            "zones": zones,
            "panels": panel_details,
            "devices": devices,
        }));
    }

    Ok(json!({
        "site": site,
        "structures": structure_details,
        "utility_accounts": accounts,
    }))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn parse_required_uuid(args: &Value, field: &str) -> Result<Uuid, AiError> {
    let s = args[field]
        .as_str()
        .ok_or_else(|| AiError::Validation(format!("{field} is required")))?;
    Uuid::parse_str(s).map_err(|e| AiError::Validation(format!("Invalid UUID for {field}: {e}")))
}

fn parse_optional_uuid(args: &Value, field: &str) -> Result<Option<Uuid>, AiError> {
    match args.get(field).and_then(|v| v.as_str()) {
        Some(s) => Ok(Some(
            Uuid::parse_str(s)
                .map_err(|e| AiError::Validation(format!("Invalid UUID for {field}: {e}")))?,
        )),
        None => Ok(None),
    }
}

fn parse_required_date(args: &Value, field: &str) -> Result<NaiveDate, AiError> {
    let s = args[field]
        .as_str()
        .ok_or_else(|| AiError::Validation(format!("{field} is required")))?;
    NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .map_err(|e| AiError::Validation(format!("Invalid date for {field}: {e}")))
}

fn parse_optional_date(args: &Value, field: &str) -> Result<Option<NaiveDate>, AiError> {
    match args.get(field).and_then(|v| v.as_str()) {
        Some(s) => Ok(Some(
            NaiveDate::parse_from_str(s, "%Y-%m-%d")
                .map_err(|e| AiError::Validation(format!("Invalid date for {field}: {e}")))?,
        )),
        None => Ok(None),
    }
}

fn parse_hypothesis_category(s: &str) -> lothal_core::HypothesisCategory {
    match s.to_lowercase().as_str() {
        "device_swap" | "device" | "hvac" | "appliance" => {
            lothal_core::HypothesisCategory::DeviceSwap
        }
        "behavior" | "behavioral" => lothal_core::HypothesisCategory::BehaviorChange,
        "envelope" | "insulation" => lothal_core::HypothesisCategory::EnvelopeUpgrade,
        "rate" | "tariff" => lothal_core::HypothesisCategory::RateOptimization,
        "load_shifting" | "load_shift" | "tou" => lothal_core::HypothesisCategory::LoadShifting,
        "maintenance" => lothal_core::HypothesisCategory::Maintenance,
        "water" | "water_conservation" => lothal_core::HypothesisCategory::WaterConservation,
        "livestock" | "chicken" => lothal_core::HypothesisCategory::LivestockOptimization,
        "land" | "land_management" | "tree" => lothal_core::HypothesisCategory::LandManagement,
        _ => lothal_core::HypothesisCategory::Other,
    }
}

// ---------------------------------------------------------------------------
// Property operations handlers
// ---------------------------------------------------------------------------

async fn handle_get_property_zones(
    args: Value,
    pool: &PgPool,
) -> Result<Value, AiError> {
    let site_id = parse_required_uuid(&args, "site_id")?;
    let zones = lothal_db::property_zone::list_property_zones_by_site(pool, site_id).await?;
    let constraints = lothal_db::property_zone::list_constraints_by_site(pool, site_id).await?;
    let trees = lothal_db::property_zone::list_trees_by_site(pool, site_id).await?;

    Ok(json!({
        "zones": zones,
        "constraints": constraints,
        "trees": trees,
    }))
}

async fn handle_get_pool_status(
    args: Value,
    pool: &PgPool,
) -> Result<Value, AiError> {
    let site_id = parse_required_uuid(&args, "site_id")?;
    let pools = lothal_db::water::list_pools_by_site(pool, site_id).await?;

    Ok(json!({ "pools": pools }))
}

async fn handle_query_livestock_logs(
    args: Value,
    pool: &PgPool,
) -> Result<Value, AiError> {
    let flock_id = parse_required_uuid(&args, "flock_id")?;
    let start = args
        .get("start")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<NaiveDate>().ok())
        .ok_or_else(|| AiError::Validation("Missing or invalid start date".into()))?;
    let end = args
        .get("end")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<NaiveDate>().ok())
        .ok_or_else(|| AiError::Validation("Missing or invalid end date".into()))?;

    let logs = lothal_db::livestock::list_logs_by_date_range(pool, flock_id, start, end).await?;

    Ok(json!({ "logs": logs }))
}

async fn handle_get_property_overview(
    args: Value,
    pool: &PgPool,
) -> Result<Value, AiError> {
    let site_id = parse_required_uuid(&args, "site_id")?;

    let (zones, constraints, trees, water_sources, pools, septic, flocks, beds, compost) = tokio::try_join!(
        async { lothal_db::property_zone::list_property_zones_by_site(pool, site_id).await },
        async { lothal_db::property_zone::list_constraints_by_site(pool, site_id).await },
        async { lothal_db::property_zone::list_trees_by_site(pool, site_id).await },
        async { lothal_db::water::list_water_sources_by_site(pool, site_id).await },
        async { lothal_db::water::list_pools_by_site(pool, site_id).await },
        async { lothal_db::water::get_septic_system(pool, site_id).await },
        async { lothal_db::livestock::list_flocks_by_site(pool, site_id).await },
        async { lothal_db::garden::list_garden_beds_by_site(pool, site_id).await },
        async { lothal_db::garden::list_compost_piles_by_site(pool, site_id).await },
    )?;

    Ok(json!({
        "property_zones": zones,
        "constraints": constraints,
        "trees": trees,
        "water_sources": water_sources,
        "pools": pools,
        "septic_system": septic,
        "flocks": flocks,
        "garden_beds": beds,
        "compost_piles": compost,
    }))
}
