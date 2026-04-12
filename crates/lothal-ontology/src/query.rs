//! Read-side helpers that hydrate views of ontology state.

use crate::{EventRecord, LinkRecord, ObjectRecord, ObjectUri};

pub struct ObjectView {
    pub object: ObjectRecord,
    pub neighbors: Vec<(LinkRecord, ObjectRecord)>,
    pub recent_events: Vec<EventRecord>,
    pub applicable_actions: Vec<String>,
}

#[derive(Default)]
pub struct ViewOptions {
    pub event_limit: usize,
    pub neighbor_depth: u8,
}

pub async fn get_object_view(
    pool: &sqlx::PgPool,
    uri: &ObjectUri,
    opts: ViewOptions,
) -> Result<ObjectView, sqlx::Error> {
    let _ = (pool, uri, opts);
    todo!()
}

pub async fn neighbors(
    pool: &sqlx::PgPool,
    uri: &ObjectUri,
    link_kind: Option<&str>,
) -> Result<Vec<(LinkRecord, ObjectRecord)>, sqlx::Error> {
    let _ = (pool, uri, link_kind);
    todo!()
}

pub async fn events_for(
    pool: &sqlx::PgPool,
    uris: &[ObjectUri],
    t0: chrono::DateTime<chrono::Utc>,
    t1: chrono::DateTime<chrono::Utc>,
    kind: Option<&str>,
) -> Result<Vec<EventRecord>, sqlx::Error> {
    let _ = (pool, uris, t0, t1, kind);
    todo!()
}

pub async fn timeline(
    pool: &sqlx::PgPool,
    uri: &ObjectUri,
    t0: chrono::DateTime<chrono::Utc>,
    t1: chrono::DateTime<chrono::Utc>,
) -> Result<Vec<EventRecord>, sqlx::Error> {
    let _ = (pool, uri, t0, t1);
    todo!()
}

pub async fn search(
    pool: &sqlx::PgPool,
    query: &str,
    kind: Option<&str>,
    limit: usize,
) -> Result<Vec<ObjectRecord>, sqlx::Error> {
    let _ = (pool, query, kind, limit);
    todo!()
}
