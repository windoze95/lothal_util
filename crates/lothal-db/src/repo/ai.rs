use chrono::{DateTime, NaiveDate, Utc};
use sqlx::PgPool;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Briefings
// ---------------------------------------------------------------------------

pub struct Briefing {
    pub id: Uuid,
    pub site_id: Uuid,
    pub date: NaiveDate,
    pub content: String,
    pub context: Option<serde_json::Value>,
    pub model: Option<String>,
    pub created_at: DateTime<Utc>,
}

pub async fn get_briefing(
    pool: &PgPool,
    site_id: Uuid,
    date: NaiveDate,
) -> Result<Option<Briefing>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT id, site_id, date, content, context, model, created_at
         FROM briefings WHERE site_id = $1 AND date = $2",
    )
    .bind(site_id)
    .bind(date)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| briefing_from_row(&r)))
}

pub async fn list_briefings(
    pool: &PgPool,
    site_id: Uuid,
    limit: i64,
) -> Result<Vec<Briefing>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, site_id, date, content, context, model, created_at
         FROM briefings WHERE site_id = $1 ORDER BY date DESC LIMIT $2",
    )
    .bind(site_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(rows.iter().map(briefing_from_row).collect())
}

fn briefing_from_row(row: &sqlx::postgres::PgRow) -> Briefing {
    use sqlx::Row;
    Briefing {
        id: row.get("id"),
        site_id: row.get("site_id"),
        date: row.get("date"),
        content: row.get("content"),
        context: row.get("context"),
        model: row.get("model"),
        created_at: row.get("created_at"),
    }
}

// ---------------------------------------------------------------------------
// Device Labels (NILM)
// ---------------------------------------------------------------------------

pub struct DeviceLabelRow {
    pub id: Uuid,
    pub circuit_id: Uuid,
    pub device_kind: String,
    pub confidence: f64,
    pub reasoning: Option<String>,
    pub signature: serde_json::Value,
    pub model: Option<String>,
    pub is_confirmed: bool,
    pub created_at: DateTime<Utc>,
}

pub async fn list_device_labels_by_circuit(
    pool: &PgPool,
    circuit_id: Uuid,
) -> Result<Vec<DeviceLabelRow>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, circuit_id, device_kind, confidence, reasoning,
                signature, model, is_confirmed, created_at
         FROM device_labels WHERE circuit_id = $1 ORDER BY created_at DESC",
    )
    .bind(circuit_id)
    .fetch_all(pool)
    .await?;

    Ok(rows.iter().map(device_label_from_row).collect())
}

pub async fn confirm_device_label(
    pool: &PgPool,
    label_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE device_labels SET is_confirmed = true WHERE id = $1")
        .bind(label_id)
        .execute(pool)
        .await?;
    Ok(())
}

fn device_label_from_row(row: &sqlx::postgres::PgRow) -> DeviceLabelRow {
    use sqlx::Row;
    DeviceLabelRow {
        id: row.get("id"),
        circuit_id: row.get("circuit_id"),
        device_kind: row.get("device_kind"),
        confidence: row.get("confidence"),
        reasoning: row.get("reasoning"),
        signature: row.get("signature"),
        model: row.get("model"),
        is_confirmed: row.get("is_confirmed"),
        created_at: row.get("created_at"),
    }
}

// ---------------------------------------------------------------------------
// Email Ingest Log
// ---------------------------------------------------------------------------

pub struct EmailIngestRow {
    pub id: Uuid,
    pub message_id: String,
    pub sender: String,
    pub subject: Option<String>,
    pub received_at: Option<DateTime<Utc>>,
    pub bill_id: Option<Uuid>,
    pub status: String,
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
}

pub async fn insert_email_ingest_log(
    pool: &PgPool,
    message_id: &str,
    sender: &str,
    subject: Option<&str>,
    bill_id: Option<Uuid>,
    status: &str,
    error: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"INSERT INTO email_ingest_log (id, message_id, sender, subject,
                                          bill_id, status, error, created_at)
           VALUES ($1, $2, $3, $4, $5, $6, $7, now())
           ON CONFLICT (message_id) DO NOTHING"#,
    )
    .bind(Uuid::new_v4())
    .bind(message_id)
    .bind(sender)
    .bind(subject)
    .bind(bill_id)
    .bind(status)
    .bind(error)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list_email_ingest_log(
    pool: &PgPool,
    limit: i64,
) -> Result<Vec<EmailIngestRow>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, message_id, sender, subject, received_at, bill_id,
                status, error, created_at
         FROM email_ingest_log ORDER BY created_at DESC LIMIT $1",
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(rows.iter().map(email_ingest_from_row).collect())
}

fn email_ingest_from_row(row: &sqlx::postgres::PgRow) -> EmailIngestRow {
    use sqlx::Row;
    EmailIngestRow {
        id: row.get("id"),
        message_id: row.get("message_id"),
        sender: row.get("sender"),
        subject: row.get("subject"),
        received_at: row.get("received_at"),
        bill_id: row.get("bill_id"),
        status: row.get("status"),
        error: row.get("error"),
        created_at: row.get("created_at"),
    }
}
