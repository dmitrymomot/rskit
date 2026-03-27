use serde::Deserialize;

/// Top-level configuration for the job worker.
///
/// Deserializes from YAML under the `job` key. All fields have defaults so an
/// empty config block is valid.
///
/// # Defaults
///
/// | Field | Default |
/// |---|---|
/// | `poll_interval_secs` | `1` |
/// | `stale_threshold_secs` | `600` (10 min) |
/// | `stale_reaper_interval_secs` | `60` (1 min) |
/// | `drain_timeout_secs` | `30` |
/// | `queues` | one `"default"` queue with concurrency 4 |
/// | `cleanup` | enabled, 1 h interval, 72 h retention |
#[non_exhaustive]
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct JobConfig {
    /// How often the worker polls the database for new jobs, in seconds.
    pub poll_interval_secs: u64,
    /// Jobs stuck in `running` for longer than this many seconds are considered
    /// stale and reset to `pending` by the reaper.
    pub stale_threshold_secs: u64,
    /// How often the stale reaper runs, in seconds.
    pub stale_reaper_interval_secs: u64,
    /// Maximum time in seconds to wait for in-flight jobs to finish during
    /// graceful shutdown.
    pub drain_timeout_secs: u64,
    /// Queue definitions. Defaults to a single `"default"` queue.
    pub queues: Vec<QueueConfig>,
    /// Optional periodic cleanup of terminal jobs. Set to `None` to disable.
    pub cleanup: Option<CleanupConfig>,
    /// Separate SQLite database for the job queue. When set, the job worker
    /// uses this pool instead of the main application database, keeping
    /// job-queue writes from contending with app queries.
    pub database: Option<crate::db::Config>,
}

impl Default for JobConfig {
    fn default() -> Self {
        Self {
            poll_interval_secs: 1,
            stale_threshold_secs: 600,
            stale_reaper_interval_secs: 60,
            drain_timeout_secs: 30,
            queues: vec![QueueConfig::default()],
            cleanup: Some(CleanupConfig::default()),
            database: None,
        }
    }
}

/// Configuration for a single named queue.
#[non_exhaustive]
#[derive(Debug, Clone, Deserialize)]
pub struct QueueConfig {
    /// Queue name. Must match the `queue` field used when enqueuing jobs.
    pub name: String,
    /// Maximum number of jobs from this queue that run concurrently.
    /// Defaults to `4`.
    #[serde(default = "default_concurrency")]
    pub concurrency: u32,
}

fn default_concurrency() -> u32 {
    4
}

impl Default for QueueConfig {
    fn default() -> Self {
        Self {
            name: "default".to_string(),
            concurrency: 4,
        }
    }
}

/// Configuration for the periodic cleanup of terminal jobs.
///
/// Terminal jobs (status `completed`, `dead`, or `cancelled`) whose
/// `updated_at` is older than `retention_secs` are deleted from the database.
#[non_exhaustive]
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct CleanupConfig {
    /// How often the cleanup task runs, in seconds. Defaults to `3600` (1 h).
    pub interval_secs: u64,
    /// Jobs whose `updated_at` is older than this many seconds are deleted.
    /// Defaults to `259200` (72 h).
    pub retention_secs: u64,
}

impl Default for CleanupConfig {
    fn default() -> Self {
        Self {
            interval_secs: 3600,
            retention_secs: 259_200, // 72h
        }
    }
}
