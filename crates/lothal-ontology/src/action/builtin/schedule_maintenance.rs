//! `schedule_maintenance` — insert a row into `maintenance_events` for each
//! subject and emit a `maintenance_scheduled` ontology event, all inside a
//! single transaction per subject.
//!
//! The INSERT column list mirrors
//! [`lothal-db::repo::maintenance::insert_maintenance_event`] to keep the
//! action and the typed repo aligned.

use async_trait::async_trait;
use serde_json::json;
use uuid::Uuid;

use crate::action::{Action, ActionCtx, ActionError};
use crate::{indexer, EventSpec, LinkSpec, ObjectRef};

use super::subjects_from_input;

pub struct ScheduleMaintenance;

#[async_trait]
impl Action for ScheduleMaintenance {
    fn name(&self) -> &'static str {
        "schedule_maintenance"
    }

    fn description(&self) -> &'static str {
        "Schedule a maintenance event on a device / structure / pool / zone."
    }

    fn applicable_kinds(&self) -> &'static [&'static str] {
        &["device", "structure", "pool", "property_zone"]
    }

    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "required": ["event_type", "description"],
            "properties": {
                "event_type": {"type": "string"},
                "description": {"type": "string"},
                "scheduled_for": {"type": "string", "format": "date-time"}
            }
        })
    }

    fn output_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "required": ["maintenance_event_ids"],
            "properties": {
                "maintenance_event_ids": {
                    "type": "array",
                    "items": {"type": "string", "format": "uuid"}
                }
            }
        })
    }

    async fn run(
        &self,
        ctx: &ActionCtx,
        input: serde_json::Value,
    ) -> Result<serde_json::Value, ActionError> {
        let event_type = required_str(&input, "event_type")?;
        let description = required_str(&input, "description")?;
        // Optional; invalid/missing falls back to today.
        let scheduled_for_date = input
            .get("scheduled_for")
            .and_then(|v| v.as_str())
            .and_then(parse_date_loose)
            .unwrap_or_else(|| chrono::Utc::now().date_naive());

        let subjects = subjects_from_input(&input)?;
        if subjects.is_empty() {
            return Err(ActionError::InvalidInput(
                "schedule_maintenance requires at least one subject".into(),
            ));
        }

        let mut ids: Vec<Uuid> = Vec::with_capacity(subjects.len());
        for subj in &subjects {
            ids.push(schedule_for_subject(ctx, subj, &event_type, &description, scheduled_for_date).await?);
        }
        Ok(json!({ "maintenance_event_ids": ids }))
    }
}

fn required_str(input: &serde_json::Value, key: &str) -> Result<String, ActionError> {
    input
        .get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| ActionError::InvalidInput(format!("{key} is required")))
}

/// Insert one `maintenance_events` row, mirror to `objects`, link it, and
/// emit the business event — all in a single transaction.
async fn schedule_for_subject(
    ctx: &ActionCtx,
    subject: &ObjectRef,
    event_type: &str,
    description: &str,
    date: chrono::NaiveDate,
) -> Result<Uuid, ActionError> {
    let mut tx = ctx.pool.begin().await?;
    // Id is generated client-side so the emitted event can reference it.
    // Columns mirror `lothal-db::repo::maintenance::insert_maintenance_event`.
    let me_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO maintenance_events
               (id, target_type, target_id, date, event_type, description,
                cost, provider, next_due, notes, created_at)
           VALUES ($1, $2, $3, $4, $5, $6, NULL, NULL, NULL, NULL, now())"#,
    )
    .bind(me_id)
    .bind(&subject.kind)
    .bind(subject.id)
    .bind(date)
    .bind(event_type)
    .bind(description)
    .execute(&mut *tx)
    .await?;

    // Mirror to `objects` so graph queries can reach the maintenance event.
    // The typed `Describe` lives in lothal-core, so the upsert is manual.
    sqlx::query(
        r#"INSERT INTO objects (kind, id, display_name, site_id, properties, updated_at)
           VALUES ('maintenance_event', $1, $2, NULL, $3, now())
           ON CONFLICT (kind, id) DO UPDATE SET
               display_name = EXCLUDED.display_name,
               properties   = EXCLUDED.properties,
               updated_at   = now(),
               deleted_at   = NULL"#,
    )
    .bind(me_id)
    .bind(format!("{event_type}: {description}"))
    .bind(sqlx::types::Json(json!({
        "target_type": subject.kind,
        "target_id": subject.id,
        "date": date.to_string(),
        "event_type": event_type,
        "description": description,
    })))
    .execute(&mut *tx)
    .await?;

    // Link maintenance_event -> target so neighbor queries reach it from either side.
    indexer::upsert_link(
        &mut tx,
        LinkSpec::new(
            "targets",
            ObjectRef::new("maintenance_event", me_id),
            subject.clone(),
        ),
    )
    .await?;

    // Business event; distinct from the generic action_completed the registry emits.
    indexer::emit_event(
        &mut tx,
        EventSpec {
            kind: "maintenance_scheduled".into(),
            site_id: None,
            subjects: vec![subject.clone(), ObjectRef::new("maintenance_event", me_id)],
            summary: format!("scheduled {event_type}: {description}"),
            severity: Some("info".into()),
            properties: json!({
                "maintenance_event_id": me_id,
                "event_type": event_type,
                "description": description,
                "date": date.to_string(),
            }),
            source: "action:schedule_maintenance".into(),
        },
    )
    .await?;

    tx.commit().await?;
    Ok(me_id)
}

/// Accept either `YYYY-MM-DD` or an RFC3339 datetime. Returns `None` for
/// anything unparseable so the caller can fall back to "today".
fn parse_date_loose(s: &str) -> Option<chrono::NaiveDate> {
    if let Ok(d) = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return Some(d);
    }
    chrono::DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|dt| dt.with_timezone(&chrono::Utc).date_naive())
}
