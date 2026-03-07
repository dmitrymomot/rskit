#[modo_db::entity(table = "modo_jobs")]
#[entity(timestamps, framework)]
#[entity(index(columns = ["state", "queue", "run_at", "priority"]))]
pub struct Job {
    #[entity(primary_key, auto = "ulid")]
    pub id: String,
    pub name: String,
    pub queue: String,
    #[entity(column_type = "Text")]
    pub payload: String,
    pub state: String,
    pub priority: i32,
    pub attempts: i32,
    pub max_attempts: i32,
    pub run_at: modo_db::chrono::DateTime<modo_db::chrono::Utc>,
    pub timeout_secs: i32,
    pub locked_by: Option<String>,
    pub locked_at: Option<modo_db::chrono::DateTime<modo_db::chrono::Utc>>,
}
