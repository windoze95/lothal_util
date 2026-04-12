use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;

use crate::error::WebError;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/site", get(site_overview))
        .route("/api/v1/devices", get(list_devices))
        .route("/api/v1/bills", get(list_bills))
        .route("/api/v1/recommendations", get(list_recommendations))
        .route("/api/v1/property", get(property_overview))
}

async fn site_overview(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, WebError> {
    let sites = lothal_db::site::list_sites(&state.pool).await?;
    match sites.into_iter().next() {
        Some(s) => {
            let structures = lothal_db::site::get_structures_by_site(&state.pool, s.id).await?;
            Ok(Json(serde_json::json!({
                "site": {
                    "id": s.id,
                    "address": s.address,
                    "climate_zone": s.climate_zone,
                },
                "structures": structures.len(),
            })))
        }
        None => Ok(Json(serde_json::json!({ "site": null }))),
    }
}

#[derive(Deserialize)]
pub struct DeviceQuery {
    pub structure_id: Option<uuid::Uuid>,
}

async fn list_devices(
    State(state): State<AppState>,
    Query(query): Query<DeviceQuery>,
) -> Result<Json<serde_json::Value>, WebError> {
    let devices = if let Some(sid) = query.structure_id {
        lothal_db::device::list_devices_by_structure(&state.pool, sid).await?
    } else {
        let sites = lothal_db::site::list_sites(&state.pool).await?;
        if let Some(s) = sites.into_iter().next() {
            let structures = lothal_db::site::get_structures_by_site(&state.pool, s.id).await?;
            let mut all = Vec::new();
            for st in &structures {
                let mut devs = lothal_db::device::list_devices_by_structure(&state.pool, st.id).await?;
                all.append(&mut devs);
            }
            all
        } else {
            Vec::new()
        }
    };

    let device_json: Vec<serde_json::Value> = devices
        .iter()
        .map(|d| {
            serde_json::json!({
                "id": d.id,
                "name": d.name,
                "kind": format!("{:?}", d.kind),
                "nameplate_watts": d.nameplate_watts,
                "estimated_daily_hours": d.estimated_daily_hours,
            })
        })
        .collect();

    Ok(Json(serde_json::json!({ "devices": device_json })))
}

#[derive(Deserialize)]
pub struct BillQuery {
    pub account_id: Option<uuid::Uuid>,
}

async fn list_bills(
    State(state): State<AppState>,
    Query(query): Query<BillQuery>,
) -> Result<Json<serde_json::Value>, WebError> {
    let bills = if let Some(aid) = query.account_id {
        lothal_db::bill::list_bills_by_account(&state.pool, aid).await.unwrap_or_default()
    } else {
        let sites = lothal_db::site::list_sites(&state.pool).await?;
        if let Some(s) = sites.into_iter().next() {
            let accounts = lothal_db::bill::list_utility_accounts_by_site(&state.pool, s.id).await?;
            let mut all = Vec::new();
            for acct in &accounts {
                let mut ab = lothal_db::bill::list_bills_by_account(&state.pool, acct.id).await.unwrap_or_default();
                all.append(&mut ab);
            }
            all
        } else {
            Vec::new()
        }
    };

    let bill_json: Vec<serde_json::Value> = bills
        .iter()
        .map(|b| {
            serde_json::json!({
                "id": b.id,
                "period_start": b.period.range.start,
                "period_end": b.period.range.end,
                "total_usage": b.total_usage,
                "total_amount": b.total_amount.value(),
            })
        })
        .collect();

    Ok(Json(serde_json::json!({ "bills": bill_json })))
}

async fn list_recommendations(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, WebError> {
    let sites = lothal_db::site::list_sites(&state.pool).await?;
    let recs = if let Some(s) = sites.into_iter().next() {
        let summaries = crate::routes::pages::build_recommendations(&state.pool, s.id)
            .await
            .unwrap_or_default();
        summaries
            .into_iter()
            .map(|r| {
                serde_json::json!({
                    "title": r.title,
                    "category": r.category,
                    "annual_savings": r.annual_savings,
                    "payback_years": r.payback_years,
                    "confidence": r.confidence,
                    "description": r.description,
                })
            })
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };

    Ok(Json(serde_json::json!({ "recommendations": recs })))
}

async fn property_overview(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, WebError> {
    let sites = lothal_db::site::list_sites(&state.pool).await?;
    match sites.into_iter().next() {
        Some(s) => {
            let zones = lothal_db::property_zone::list_property_zones_by_site(&state.pool, s.id).await.unwrap_or_default();
            let pools = lothal_db::water::list_pools_by_site(&state.pool, s.id).await.unwrap_or_default();

            Ok(Json(serde_json::json!({
                "zones": zones.len(),
                "pools": pools.len(),
            })))
        }
        None => Ok(Json(serde_json::json!({}))),
    }
}
