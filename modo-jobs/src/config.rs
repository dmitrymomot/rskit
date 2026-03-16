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
    /// How often the stale reaper checks for stale jobs (seconds).
    pub stale_reaper_interval_secs: u64,
    /// Maximum time to wait for in-flight jobs during shutdown (seconds).
    pub drain_timeout_secs: u64,
    /// Per-queue concurrency configuration.
    pub queues: Vec<QueueConfig>,
    /// Auto-cleanup configuration for finished jobs.
    pub cleanup: CleanupConfig,
    /// Optional maximum payload size in bytes. `None` means unlimited.
    pub max_payload_bytes: Option<usize>,
}

impl JobsConfig {
    /// Validate the configuration, returning an error if any invariant is violated.
    ///
    /// Checks that `poll_interval_secs`, `stale_threshold_secs`,
    /// `stale_reaper_interval_secs`, and `cleanup.interval_secs` are all greater
    /// than zero, that at least one queue is configured, and that every queue has
    /// `concurrency > 0`.
    pub fn validate(&self) -> Result<(), Error> {
        if self.poll_interval_secs == 0 {
            return Err(Error::internal("poll_interval_secs must be > 0"));
        }
        if self.stale_threshold_secs == 0 {
            return Err(Error::internal("stale_threshold_secs must be > 0"));
        }
        if self.stale_reaper_interval_secs == 0 {
            return Err(Error::internal("stale_reaper_interval_secs must be > 0"));
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
            stale_reaper_interval_secs: 60,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_60s_stale_reaper_interval() {
        let config = JobsConfig::default();
        assert_eq!(config.stale_reaper_interval_secs, 60);
    }

    #[test]
    fn validate_rejects_zero_stale_reaper_interval() {
        let config = JobsConfig {
            stale_reaper_interval_secs: 0,
            ..Default::default()
        };
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("stale_reaper_interval_secs"));
    }

    #[test]
    fn validate_accepts_nonzero_stale_reaper_interval() {
        let config = JobsConfig {
            stale_reaper_interval_secs: 30,
            ..Default::default()
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn config_deserializes_stale_reaper_interval() {
        let yaml = r#"
            stale_reaper_interval_secs: 120
        "#;
        let config: JobsConfig = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(config.stale_reaper_interval_secs, 120);
    }
}
