use std::path::Path;

use anyhow::{bail, Context, Result};
use sqlx::PgPool;
use uuid::Uuid;

/// Import GeoJSON geometry (site boundary, structure footprints, zone shapes)
/// from a FeatureCollection file. All updates run in a single transaction.
///
/// Each feature must have `properties.target`, one of:
///   * `site_boundary`         — updates sites.boundary WHERE id = <site_id>
///   * `structure:<uuid>`      — updates structures.footprint WHERE id = <uuid>
///   * `zone:<uuid>`           — updates property_zones.shape WHERE id = <uuid>
///
/// The Feature's `geometry` sub-object is stored verbatim as JSONB.
pub async fn import(pool: &PgPool, site_id: &str, path: &str) -> Result<()> {
    let site_id = Uuid::parse_str(site_id)
        .with_context(|| format!("invalid --site UUID: {}", site_id))?;

    let file_path = Path::new(path);
    if !file_path.exists() {
        bail!("File not found: {}", path);
    }

    let raw = std::fs::read_to_string(file_path)
        .with_context(|| format!("failed to read {}", path))?;
    let root: serde_json::Value = serde_json::from_str(&raw)
        .with_context(|| format!("failed to parse {} as JSON", path))?;

    // Validate top-level FeatureCollection.
    let top_type = root
        .get("type")
        .and_then(|t| t.as_str())
        .unwrap_or("");
    if top_type != "FeatureCollection" {
        bail!(
            "expected GeoJSON FeatureCollection, got type = {:?}",
            top_type
        );
    }
    let features = root
        .get("features")
        .and_then(|f| f.as_array())
        .ok_or_else(|| anyhow::anyhow!("FeatureCollection missing 'features' array"))?;

    let mut tx = pool.begin().await?;
    let mut site_count: u64 = 0;
    let mut structure_count: u64 = 0;
    let mut zone_count: u64 = 0;

    for (idx, feature) in features.iter().enumerate() {
        let target = match feature
            .get("properties")
            .and_then(|p| p.get("target"))
            .and_then(|t| t.as_str())
        {
            Some(s) => s,
            None => {
                eprintln!(
                    "warning: feature #{} missing properties.target — skipping",
                    idx
                );
                continue;
            }
        };

        let geometry = feature.get("geometry").ok_or_else(|| {
            anyhow::anyhow!("feature #{} ({}) missing geometry", idx, target)
        })?;

        // Warn (but accept) non-Polygon geometries; the renderer handles them.
        let geom_type = geometry
            .get("type")
            .and_then(|t| t.as_str())
            .unwrap_or("");
        match geom_type {
            "Polygon" => {}
            "MultiPolygon" => {
                eprintln!(
                    "note: feature #{} ({}) has MultiPolygon geometry",
                    idx, target
                );
            }
            other => {
                eprintln!(
                    "warning: feature #{} ({}) has unexpected geometry type {:?} — storing anyway",
                    idx, target, other
                );
            }
        }

        if target == "site_boundary" {
            let result = sqlx::query("UPDATE sites SET boundary = $2 WHERE id = $1")
                .bind(site_id)
                .bind(geometry.clone())
                .execute(&mut *tx)
                .await
                .with_context(|| {
                    format!("feature #{}: failed to update site boundary", idx)
                })?;
            if result.rows_affected() == 0 {
                tx.rollback().await.ok();
                bail!(
                    "feature #{}: no site with id {} (rolling back)",
                    idx, site_id
                );
            }
            site_count += result.rows_affected();
        } else if let Some(rest) = target.strip_prefix("structure:") {
            let structure_id = Uuid::parse_str(rest).with_context(|| {
                format!("feature #{}: invalid structure uuid {:?}", idx, rest)
            })?;
            let result =
                sqlx::query("UPDATE structures SET footprint = $2 WHERE id = $1")
                    .bind(structure_id)
                    .bind(geometry.clone())
                    .execute(&mut *tx)
                    .await
                    .with_context(|| {
                        format!(
                            "feature #{}: failed to update structure {}",
                            idx, structure_id
                        )
                    })?;
            if result.rows_affected() == 0 {
                tx.rollback().await.ok();
                bail!(
                    "feature #{}: no structure with id {} (rolling back)",
                    idx, structure_id
                );
            }
            structure_count += result.rows_affected();
        } else if let Some(rest) = target.strip_prefix("zone:") {
            let zone_id = Uuid::parse_str(rest).with_context(|| {
                format!("feature #{}: invalid zone uuid {:?}", idx, rest)
            })?;
            let result =
                sqlx::query("UPDATE property_zones SET shape = $2 WHERE id = $1")
                    .bind(zone_id)
                    .bind(geometry.clone())
                    .execute(&mut *tx)
                    .await
                    .with_context(|| {
                        format!(
                            "feature #{}: failed to update zone {}",
                            idx, zone_id
                        )
                    })?;
            if result.rows_affected() == 0 {
                tx.rollback().await.ok();
                bail!(
                    "feature #{}: no property_zone with id {} (rolling back)",
                    idx, zone_id
                );
            }
            zone_count += result.rows_affected();
        } else {
            eprintln!(
                "warning: feature #{} has unknown target {:?} — skipping",
                idx, target
            );
            continue;
        }
    }

    tx.commit().await?;

    println!(
        "Updated: {} site boundary, {} structure footprints, {} zone shapes",
        site_count, structure_count, zone_count
    );
    Ok(())
}
