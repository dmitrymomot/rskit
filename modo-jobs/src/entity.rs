/// Database entity for a persisted job record.
///
/// This entity is auto-registered with `modo-db` at link time and maps to the
/// `modo_jobs` table.  The table is created and migrated automatically when
/// `modo_db::sync_and_migrate` is called.
///
/// Fields that are managed by the runner (e.g. `locked_by`, `state`) should not
/// be written directly by application code — use [`crate::JobQueue`] instead.
#[modo_db::entity(table = "modo_jobs", group = "jobs")]
#[entity(timestamps, framework)]
#[entity(index(columns = ["state", "queue", "run_at", "priority"]))]
pub struct Job {
    /// Primary key — a ULID string generated when the job is enqueued.
    #[entity(primary_key, auto = "ulid")]
    pub id: String,
    /// Registered job name (matches `#[job]` function name, e.g. `"send_welcome"`).
    pub name: String,
    /// Queue the job is dispatched to.
    pub queue: String,
    /// JSON-serialized job payload.
    #[entity(column_type = "Text")]
    pub payload: String,
    /// Current [`crate::JobState`] stored as a lowercase string.
    pub state: String,
    /// Execution priority — higher values run first within the same queue.
    pub priority: i32,
    /// Number of times this job has been attempted so far.
    pub attempts: i32,
    /// Maximum number of attempts before the job is marked `dead`.
    pub max_attempts: i32,
    /// Earliest time the job may run.
    pub run_at: modo_db::chrono::DateTime<modo_db::chrono::Utc>,
    /// Per-job execution timeout in seconds.
    pub timeout_secs: i32,
    /// Worker ID that currently holds the lock, or `None` when not running.
    pub locked_by: Option<String>,
    /// Timestamp when the current lock was acquired, or `None`.
    pub locked_at: Option<modo_db::chrono::DateTime<modo_db::chrono::Utc>>,
    /// Error message from the most recent failed attempt, if any.
    #[entity(column_type = "Text")]
    pub last_error: Option<String>,
}
