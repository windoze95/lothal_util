//! Property map (W4).
//!
//! Renders the site boundary + every structure footprint + every property
//! zone shape as a single GeoJSON FeatureCollection. Everything after this
//! handler — projection fitting, zoom, click interactions — is handled by
//! the browser-side d3 script.

use axum::extract::State;
use axum::http::header;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::WebError;
use crate::state::AppState;
use crate::templates::MapPage;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/map", get(map_page))
        .route("/map/geojson", get(map_geojson))
}

async fn map_page(State(state): State<AppState>) -> Result<MapPage, WebError> {
    let site = first_site(&state.pool).await?;
    let site_name = site
        .as_ref()
        .map(|s| s.address.clone())
        .unwrap_or_else(|| "My Property".into());

    let collection = match &site {
        Some(s) => build_feature_collection(&state.pool, s.id).await?,
        None => empty_collection(),
    };

    // Inline the GeoJSON directly as a JS literal. Using a single-string
    // embedding keeps the HTML parser oblivious to the JSON's quotes; the
    // d3 script parses it with `JSON.parse`.
    let geojson =
        serde_json::to_string(&collection).unwrap_or_else(|_| "{\"type\":\"FeatureCollection\",\"features\":[]}".into());

    Ok(MapPage {
        active_page: "map".into(),
        site_name,
        geojson,
    })
}

async fn map_geojson(State(state): State<AppState>) -> Result<Response, WebError> {
    let site = first_site(&state.pool).await?;
    let collection = match &site {
        Some(s) => build_feature_collection(&state.pool, s.id).await?,
        None => empty_collection(),
    };
    Ok((
        [(header::CONTENT_TYPE, "application/json")],
        Json(collection),
    )
        .into_response())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async fn first_site(
    pool: &PgPool,
) -> Result<Option<lothal_core::ontology::site::Site>, WebError> {
    let sites = lothal_db::site::list_sites(pool).await?;
    Ok(sites.into_iter().next())
}

fn empty_collection() -> Value {
    json!({
        "type": "FeatureCollection",
        "features": [],
    })
}

/// Build the FeatureCollection by merging three sources:
/// 1. The site boundary (one feature, `kind=site`).
/// 2. Every structure footprint on the site (`kind=structure`).
/// 3. Every property zone shape on the site (`kind=zone`).
///
/// Each source stores a GeoJSON object — usually a `Feature` or bare
/// `Geometry`. We normalize to `Feature` and stamp identifying metadata on
/// `properties` so the client can filter by layer and wire up click handlers
/// that open the universal entity page.
async fn build_feature_collection(
    pool: &PgPool,
    site_id: Uuid,
) -> Result<Value, WebError> {
    let mut features: Vec<Value> = Vec::new();

    // 1. Site boundary.
    if let Some(boundary) = lothal_db::site::get_site_boundary(pool, site_id).await? {
        features.push(normalize_feature(
            boundary,
            json!({
                "layer": "site",
                "kind": "site",
                "id": site_id,
                "name": "Site boundary",
            }),
        ));
    }

    // 2. Structure footprints.
    for (sid, name, footprint) in lothal_db::site::list_structure_footprints(pool, site_id).await? {
        features.push(normalize_feature(
            footprint,
            json!({
                "layer": "structures",
                "kind": "structure",
                "id": sid,
                "name": name,
            }),
        ));
    }

    // 3. Zone shapes.
    for (zid, name, kind, shape) in
        lothal_db::property_zone::list_zone_shapes(pool, site_id).await?
    {
        features.push(normalize_feature(
            shape,
            json!({
                "layer": "zones",
                "kind": "property_zone",
                "zone_kind": kind,
                "id": zid,
                "name": name,
            }),
        ));
    }

    Ok(json!({
        "type": "FeatureCollection",
        "features": features,
    }))
}

/// Wrap an arbitrary GeoJSON fragment as a `Feature`, merging in caller-supplied
/// metadata onto `properties` without clobbering any existing keys.
fn normalize_feature(raw: Value, mut meta: Value) -> Value {
    let mut feature = match raw {
        Value::Object(ref o) if o.get("type").and_then(|v| v.as_str()) == Some("Feature") => raw,
        // Anything else is treated as a bare Geometry.
        other => json!({
            "type": "Feature",
            "geometry": other,
            "properties": {},
        }),
    };

    // Merge meta into properties.
    let obj = feature.as_object_mut().expect("Feature is an object");
    let props = obj
        .entry("properties".to_string())
        .or_insert_with(|| json!({}));
    if !props.is_object() {
        *props = json!({});
    }
    if let Value::Object(incoming) = meta.take() {
        if let Some(m) = props.as_object_mut() {
            for (k, v) in incoming {
                m.entry(k).or_insert(v);
            }
        }
    }

    feature
}
