//! Write-side helpers that persist ontology objects, links, and events.
//!
//! All signatures take a live `sqlx::Transaction` so callers can mix ontology
//! writes with their own domain-table writes atomically.

use uuid::Uuid;

use crate::{Describe, EventSpec, LinkSpec, ObjectRef};

pub async fn upsert_object<T: Describe>(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    obj: &T,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO objects (kind, id, display_name, site_id, properties, updated_at)
        VALUES ($1, $2, $3, $4, $5, now())
        ON CONFLICT (kind, id) DO UPDATE SET
            display_name = EXCLUDED.display_name,
            site_id      = EXCLUDED.site_id,
            properties   = EXCLUDED.properties,
            updated_at   = now(),
            deleted_at   = NULL
        "#,
    )
    .bind(T::KIND)
    .bind(obj.id())
    .bind(obj.display_name())
    .bind(obj.site_id())
    .bind(sqlx::types::Json(obj.properties()))
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn soft_delete_object(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    kind: &str,
    id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        UPDATE objects
        SET deleted_at = now()
        WHERE kind = $1 AND id = $2 AND deleted_at IS NULL
        "#,
    )
    .bind(kind)
    .bind(id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn upsert_link(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    link: LinkSpec,
) -> Result<Uuid, sqlx::Error> {
    let row: (Uuid,) = sqlx::query_as(
        r#"
        INSERT INTO links (kind, src_kind, src_id, dst_kind, dst_id, properties)
        VALUES ($1, $2, $3, $4, $5, $6)
        ON CONFLICT ON CONSTRAINT uq_links_current DO UPDATE SET
            properties = EXCLUDED.properties
        RETURNING id
        "#,
    )
    .bind(&link.kind)
    .bind(&link.src.kind)
    .bind(link.src.id)
    .bind(&link.dst.kind)
    .bind(link.dst.id)
    .bind(sqlx::types::Json(link.properties))
    .fetch_one(&mut **tx)
    .await?;
    Ok(row.0)
}

pub async fn close_link(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    kind: &str,
    src: ObjectRef,
    dst: ObjectRef,
    at: chrono::DateTime<chrono::Utc>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        UPDATE links
        SET valid_until = $6
        WHERE kind = $1
          AND src_kind = $2
          AND src_id = $3
          AND dst_kind = $4
          AND dst_id = $5
          AND valid_until IS NULL
        "#,
    )
    .bind(kind)
    .bind(&src.kind)
    .bind(src.id)
    .bind(&dst.kind)
    .bind(dst.id)
    .bind(at)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn emit_event(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ev: EventSpec,
) -> Result<Uuid, sqlx::Error> {
    let subjects_json = serde_json::to_value(
        &ev.subjects
            .iter()
            .map(|r| serde_json::json!({ "kind": r.kind, "id": r.id }))
            .collect::<Vec<_>>(),
    )
    .unwrap_or(serde_json::Value::Array(Vec::new()));

    let row: (Uuid,) = sqlx::query_as(
        r#"
        INSERT INTO events (time, kind, site_id, subjects, summary, severity, properties, source)
        VALUES (now(), $1, $2, $3, $4, $5, $6, $7)
        RETURNING id
        "#,
    )
    .bind(&ev.kind)
    .bind(ev.site_id)
    .bind(sqlx::types::Json(subjects_json))
    .bind(&ev.summary)
    .bind(ev.severity.as_deref())
    .bind(sqlx::types::Json(ev.properties))
    .bind(&ev.source)
    .fetch_one(&mut **tx)
    .await?;
    Ok(row.0)
}
