/// Persisted record of a single action invocation.
pub struct ActionRun {
    pub id: uuid::Uuid,
    pub action_name: String,
    pub status: String,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub finished_at: Option<chrono::DateTime<chrono::Utc>>,
}
