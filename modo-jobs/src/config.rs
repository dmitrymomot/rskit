use crate::types::JobState;
use modo::Error;
use serde::Deserialize;

/// Top-level jobs configuration, deserialized from YAML.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct JobsConfig {
    /// How often each queue polls for new jobs (seconds).
    pub poll_interval_secs: u64,
    /// Jobs running longer than this are considered stale and re-queued (seconds).
    pub stale_threshold_secs: u64,
    /// Maximum time to wait for in-flight jobs during shutdown (seconds).
    pub drain_timeout_secs: u64,
    /// Per-queue concurrency configuration.
    pub queues: Vec<QueueConfig>,
    /// Auto-cleanup configuration for finished jobs.
    pub cleanup: CleanupConfig,
    /// Optional maximum payload size in bytes. None = unlimited.
    pub max_payload_bytes: Option<usize>,
}

impl JobsConfig {
    pub fn validate(&self) -> Result<(), Error> {
        if self.poll_interval_secs == 0 {
            return Err(Error::internal("poll_interval_secs must be > 0"));
        }
        if self.stale_threshold_secs == 0 {
            return Err(Error::internal("stale_threshold_secs must be > 0"));
        }
        if self.queues.is_empty() {
            return Err(Error::internal("at least one queue must be configured"));
        }
        for q in &self.queues {
            if q.concurrency == 0 {
                return Err(Error::internal(format!(
                    "queue '{}': concurrency must be > 0",
                    q.name
                )));
            }
        }
        if self.cleanup.interval_secs == 0 {
            return Err(Error::internal("cleanup.interval_secs must be > 0"));
        }
        Ok(())
    }
}

impl Default for JobsConfig {
    fn default() -> Self {
        Self {
            poll_interval_secs: 1,
            stale_threshold_secs: 600,
            drain_timeout_secs: 30,
            queues: vec![QueueConfig {
                name: "default".to_string(),
                concurrency: 4,
            }],
            cleanup: CleanupConfig::default(),
            max_payload_bytes: None,
        }
    }
}

/// Configuration for a single named queue.
#[derive(Debug, Clone, Deserialize)]
pub struct QueueConfig {
    /// Queue name (must match the queue name used in `#[job(queue = "...")]`).
    pub name: String,
    /// Maximum number of concurrent jobs in this queue.
    pub concurrency: usize,
}

/// Configuration for automatic cleanup of finished jobs.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct CleanupConfig {
    /// How often the cleanup task runs (seconds).
    pub interval_secs: u64,
    /// Jobs older than this are eligible for cleanup (seconds).
    pub retention_secs: u64,
    /// Which job states to clean up.
    pub statuses: Vec<JobState>,
}

impl Default for CleanupConfig {
    fn default() -> Self {
        Self {
            interval_secs: 3600,
            retention_secs: 86400,
            statuses: vec![JobState::Completed, JobState::Dead, JobState::Cancelled],
        }
    }
}
