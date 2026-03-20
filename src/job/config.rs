use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct JobConfig {
    pub poll_interval_secs: u64,
    pub stale_threshold_secs: u64,
    pub stale_reaper_interval_secs: u64,
    pub drain_timeout_secs: u64,
    pub queues: Vec<QueueConfig>,
    pub cleanup: Option<CleanupConfig>,
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
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct QueueConfig {
    pub name: String,
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

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct CleanupConfig {
    pub interval_secs: u64,
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
