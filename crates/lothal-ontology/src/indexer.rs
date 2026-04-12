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
    let _ = (tx, obj);
    todo!()
}

pub async fn soft_delete_object(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    kind: &str,
    id: Uuid,
) -> Result<(), sqlx::Error> {
    let _ = (tx, kind, id);
    todo!()
}

pub async fn upsert_link(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    link: LinkSpec,
) -> Result<Uuid, sqlx::Error> {
    let _ = (tx, link);
    todo!()
}

pub async fn close_link(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    kind: &str,
    src: ObjectRef,
    dst: ObjectRef,
    at: chrono::DateTime<chrono::Utc>,
) -> Result<(), sqlx::Error> {
    let _ = (tx, kind, src, dst, at);
    todo!()
}

pub async fn emit_event(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ev: EventSpec,
) -> Result<Uuid, sqlx::Error> {
    let _ = (tx, ev);
    todo!()
}
