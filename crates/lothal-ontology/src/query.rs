//! Read-side helpers that hydrate views of ontology state.
//!
//! Queries are deliberately composed from small, focused statements rather
//! than a single mega-join. Each query is parameterized — no user input is
//! interpolated into SQL strings.

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

/// Combined row shape returned by the bidirectional neighbor UNION ALL.
///
/// A single row pairs one link with the resolved "other side" object. We
/// alias every link column to `l_*` and every object column to `o_*` so the
/// shape is stable regardless of which side of the UNION produced the row.
#[derive(sqlx::FromRow)]
struct NeighborRow {
    l_id: uuid::Uuid,
    l_kind: String,
    l_src_kind: String,
    l_src_id: uuid::Uuid,
    l_dst_kind: String,
    l_dst_id: uuid::Uuid,
    l_valid_from: chrono::DateTime<chrono::Utc>,
    l_valid_until: Option<chrono::DateTime<chrono::Utc>>,
    l_properties: sqlx::types::Json<serde_json::Value>,
    o_kind: String,
    o_id: uuid::Uuid,
    o_display_name: String,
    o_site_id: Option<uuid::Uuid>,
    o_properties: sqlx::types::Json<serde_json::Value>,
    o_created_at: chrono::DateTime<chrono::Utc>,
    o_updated_at: chrono::DateTime<chrono::Utc>,
    o_deleted_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl NeighborRow {
    fn split(self) -> (LinkRecord, ObjectRecord) {
        let link = LinkRecord {
            id: self.l_id,
            kind: self.l_kind,
            src_kind: self.l_src_kind,
            src_id: self.l_src_id,
            dst_kind: self.l_dst_kind,
            dst_id: self.l_dst_id,
            valid_from: self.l_valid_from,
            valid_until: self.l_valid_until,
            properties: self.l_properties,
        };
        let obj = ObjectRecord {
            kind: self.o_kind,
            id: self.o_id,
            display_name: self.o_display_name,
            site_id: self.o_site_id,
            properties: self.o_properties,
            created_at: self.o_created_at,
            updated_at: self.o_updated_at,
            deleted_at: self.o_deleted_at,
        };
        (link, obj)
    }
}

/// SQL for the bidirectional neighbor UNION ALL.
///
/// `$1` = kind, `$2` = id, `$3` = link_kind filter (nullable text), `$4` =
/// LIMIT. The src-side half joins `links.dst_*` to `objects`, the dst-side
/// half joins `links.src_*` — producing a row per (link, far-side object)
/// pair regardless of direction. Both halves exclude closed links and
/// soft-deleted objects.
const NEIGHBOR_UNION_SQL: &str = r#"
    SELECT
        l.id           AS l_id,
        l.kind         AS l_kind,
        l.src_kind     AS l_src_kind,
        l.src_id       AS l_src_id,
        l.dst_kind     AS l_dst_kind,
        l.dst_id       AS l_dst_id,
        l.valid_from   AS l_valid_from,
        l.valid_until  AS l_valid_until,
        l.properties   AS l_properties,
        o.kind         AS o_kind,
        o.id           AS o_id,
        o.display_name AS o_display_name,
        o.site_id      AS o_site_id,
        o.properties   AS o_properties,
        o.created_at   AS o_created_at,
        o.updated_at   AS o_updated_at,
        o.deleted_at   AS o_deleted_at
    FROM links l
    JOIN objects o
      ON o.kind = l.dst_kind
     AND o.id   = l.dst_id
    WHERE l.src_kind = $1
      AND l.src_id   = $2
      AND l.valid_until IS NULL
      AND ($3::text IS NULL OR l.kind = $3)
      AND o.deleted_at IS NULL
    UNION ALL
    SELECT
        l.id           AS l_id,
        l.kind         AS l_kind,
        l.src_kind     AS l_src_kind,
        l.src_id       AS l_src_id,
        l.dst_kind     AS l_dst_kind,
        l.dst_id       AS l_dst_id,
        l.valid_from   AS l_valid_from,
        l.valid_until  AS l_valid_until,
        l.properties   AS l_properties,
        o.kind         AS o_kind,
        o.id           AS o_id,
        o.display_name AS o_display_name,
        o.site_id      AS o_site_id,
        o.properties   AS o_properties,
        o.created_at   AS o_created_at,
        o.updated_at   AS o_updated_at,
        o.deleted_at   AS o_deleted_at
    FROM links l
    JOIN objects o
      ON o.kind = l.src_kind
     AND o.id   = l.src_id
    WHERE l.dst_kind = $1
      AND l.dst_id   = $2
      AND l.valid_until IS NULL
      AND ($3::text IS NULL OR l.kind = $3)
      AND o.deleted_at IS NULL
    LIMIT $4
"#;

pub async fn get_object_view(
    pool: &sqlx::PgPool,
    uri: &ObjectUri,
    opts: ViewOptions,
) -> Result<ObjectView, sqlx::Error> {
    // 1. Object row (or RowNotFound).
    let object: ObjectRecord = sqlx::query_as(
        r#"
        SELECT kind, id, display_name, site_id, properties,
               created_at, updated_at, deleted_at
        FROM objects
        WHERE kind = $1 AND id = $2 AND deleted_at IS NULL
        "#,
    )
    .bind(&uri.kind)
    .bind(uri.id)
    .fetch_one(pool)
    .await?;

    // 2. Neighbors — depth 1, both directions, no kind filter here.
    let neighbor_rows: Vec<NeighborRow> = sqlx::query_as(NEIGHBOR_UNION_SQL)
        .bind(&uri.kind)
        .bind(uri.id)
        .bind(Option::<&str>::None)
        .bind(500_i64)
        .fetch_all(pool)
        .await?;
    let neighbors: Vec<(LinkRecord, ObjectRecord)> =
        neighbor_rows.into_iter().map(NeighborRow::split).collect();

    // 3. Recent events where this URI appears as a subject.
    let event_limit = if opts.event_limit == 0 {
        25
    } else {
        opts.event_limit.min(500)
    };
    let recent_events: Vec<EventRecord> = sqlx::query_as(
        r#"
        SELECT time, id, kind, site_id, subjects, summary, severity, properties, source
        FROM events
        WHERE subjects @> jsonb_build_array(jsonb_build_object('kind', $1::text, 'id', $2::uuid))
        ORDER BY time DESC
        LIMIT $3
        "#,
    )
    .bind(&uri.kind)
    .bind(uri.id)
    .bind(event_limit as i64)
    .fetch_all(pool)
    .await?;

    // 4. Applicable actions.
    // TODO: thread an ActionRegistry through here and return action names that
    // match this object's kind. For now the registry is not plumbed yet.
    let applicable_actions: Vec<String> = Vec::new();

    Ok(ObjectView {
        object,
        neighbors,
        recent_events,
        applicable_actions,
    })
}

pub async fn neighbors(
    pool: &sqlx::PgPool,
    uri: &ObjectUri,
    link_kind: Option<&str>,
) -> Result<Vec<(LinkRecord, ObjectRecord)>, sqlx::Error> {
    let rows: Vec<NeighborRow> = sqlx::query_as(NEIGHBOR_UNION_SQL)
        .bind(&uri.kind)
        .bind(uri.id)
        .bind(link_kind)
        .bind(500_i64)
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(NeighborRow::split).collect())
}

pub async fn events_for(
    pool: &sqlx::PgPool,
    uris: &[ObjectUri],
    t0: chrono::DateTime<chrono::Utc>,
    t1: chrono::DateTime<chrono::Utc>,
    kind: Option<&str>,
) -> Result<Vec<EventRecord>, sqlx::Error> {
    if uris.is_empty() {
        return Ok(Vec::new());
    }

    // Build a parameterized OR chain of `subjects @> $N` clauses. Each URI
    // becomes one JSON-array bind parameter shaped `[{"kind":..,"id":..}]`;
    // the containment operator `@>` matches any subjects array whose elements
    // include that object. Using `ANY(ARRAY[...])` on jsonb does not work for
    // containment, so we enumerate the OR branches explicitly.
    let mut sql = String::from(
        "SELECT time, id, kind, site_id, subjects, summary, severity, properties, source \
         FROM events WHERE (",
    );
    for i in 0..uris.len() {
        if i > 0 {
            sql.push_str(" OR ");
        }
        // $1..$N = URI-shaped JSON containment operands.
        sql.push_str(&format!("subjects @> ${}::jsonb", i + 1));
    }
    sql.push(')');

    let t0_idx = uris.len() + 1;
    let t1_idx = uris.len() + 2;
    let kind_idx = uris.len() + 3;
    sql.push_str(&format!(
        " AND time >= ${} AND time < ${} AND (${}::text IS NULL OR kind = ${}::text) \
         ORDER BY time DESC LIMIT 1000",
        t0_idx, t1_idx, kind_idx, kind_idx,
    ));

    let mut q = sqlx::query_as::<_, EventRecord>(&sql);
    for uri in uris {
        let operand = serde_json::json!([{ "kind": uri.kind, "id": uri.id }]);
        q = q.bind(operand);
    }
    q = q.bind(t0).bind(t1).bind(kind);
    q.fetch_all(pool).await
}

pub async fn timeline(
    pool: &sqlx::PgPool,
    uri: &ObjectUri,
    t0: chrono::DateTime<chrono::Utc>,
    t1: chrono::DateTime<chrono::Utc>,
) -> Result<Vec<EventRecord>, sqlx::Error> {
    events_for(pool, std::slice::from_ref(uri), t0, t1, None).await
}

pub async fn search(
    pool: &sqlx::PgPool,
    query: &str,
    kind: Option<&str>,
    limit: usize,
) -> Result<Vec<ObjectRecord>, sqlx::Error> {
    let clamped = limit.clamp(1, 200) as i64;
    sqlx::query_as(
        r#"
        SELECT kind, id, display_name, site_id, properties,
               created_at, updated_at, deleted_at
        FROM objects
        WHERE search_tsv @@ plainto_tsquery('english', $1)
          AND ($2::text IS NULL OR kind = $2)
          AND deleted_at IS NULL
        ORDER BY ts_rank(search_tsv, plainto_tsquery('english', $1)) DESC
        LIMIT $3
        "#,
    )
    .bind(query)
    .bind(kind)
    .bind(clamped)
    .fetch_all(pool)
    .await
}
