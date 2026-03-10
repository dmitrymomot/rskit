use modo_jobs::config::QueueConfig;
use modo_jobs::{CleanupConfig, JobState, JobsConfig};

#[test]
fn test_jobs_config_defaults() {
    let config = JobsConfig::default();
    assert_eq!(config.poll_interval_secs, 1);
    assert_eq!(config.stale_threshold_secs, 600);
    assert_eq!(config.drain_timeout_secs, 30);
    assert_eq!(config.queues.len(), 1);
    assert_eq!(config.queues[0].name, "default");
    assert_eq!(config.queues[0].concurrency, 4);
}

#[test]
fn test_cleanup_config_defaults() {
    let config = CleanupConfig::default();
    assert_eq!(config.interval_secs, 3600);
    assert_eq!(config.retention_secs, 86400);
    assert_eq!(
        config.statuses,
        vec![JobState::Completed, JobState::Dead, JobState::Cancelled]
    );
}

#[test]
fn test_jobs_config_deserialize_yaml() {
    let yaml = r#"
poll_interval_secs: 5
stale_threshold_secs: 300
drain_timeout_secs: 60
queues:
  - name: emails
    concurrency: 2
  - name: reports
    concurrency: 1
cleanup:
  interval_secs: 7200
  retention_secs: 172800
  statuses: [completed]
"#;
    let config: JobsConfig = serde_yaml_ng::from_str(yaml).unwrap();
    assert_eq!(config.poll_interval_secs, 5);
    assert_eq!(config.stale_threshold_secs, 300);
    assert_eq!(config.drain_timeout_secs, 60);
    assert_eq!(config.queues.len(), 2);
    assert_eq!(config.queues[0].name, "emails");
    assert_eq!(config.queues[0].concurrency, 2);
    assert_eq!(config.queues[1].name, "reports");
    assert_eq!(config.queues[1].concurrency, 1);
    assert_eq!(config.cleanup.interval_secs, 7200);
    assert_eq!(config.cleanup.retention_secs, 172800);
    assert_eq!(config.cleanup.statuses, vec![JobState::Completed]);
}

#[test]
fn test_jobs_config_partial_yaml_uses_defaults() {
    let yaml = r#"
poll_interval_secs: 10
"#;
    let config: JobsConfig = serde_yaml_ng::from_str(yaml).unwrap();
    assert_eq!(config.poll_interval_secs, 10);
    // rest should be defaults
    assert_eq!(config.stale_threshold_secs, 600);
    assert_eq!(config.drain_timeout_secs, 30);
    assert_eq!(config.queues.len(), 1);
    assert_eq!(config.queues[0].name, "default");
    assert_eq!(config.cleanup.interval_secs, 3600);
}

#[test]
fn test_default_config_validates() {
    let config = JobsConfig::default();
    assert!(config.validate().is_ok());
}

#[test]
fn test_validate_rejects_zero_poll_interval() {
    let config = JobsConfig {
        poll_interval_secs: 0,
        ..Default::default()
    };
    let err = config.validate().unwrap_err().to_string();
    assert!(err.contains("poll_interval_secs"));
}

#[test]
fn test_validate_rejects_zero_stale_threshold() {
    let config = JobsConfig {
        stale_threshold_secs: 0,
        ..Default::default()
    };
    let err = config.validate().unwrap_err().to_string();
    assert!(err.contains("stale_threshold_secs"));
}

#[test]
fn test_validate_rejects_empty_queues() {
    let config = JobsConfig {
        queues: vec![],
        ..Default::default()
    };
    let err = config.validate().unwrap_err().to_string();
    assert!(err.contains("queue"));
}

#[test]
fn test_validate_rejects_zero_concurrency() {
    let config = JobsConfig {
        queues: vec![QueueConfig {
            name: "test".to_string(),
            concurrency: 0,
        }],
        ..Default::default()
    };
    let err = config.validate().unwrap_err().to_string();
    assert!(err.contains("concurrency"));
}

#[test]
fn test_validate_rejects_zero_cleanup_interval() {
    let config = JobsConfig {
        cleanup: CleanupConfig {
            interval_secs: 0,
            ..Default::default()
        },
        ..Default::default()
    };
    let err = config.validate().unwrap_err().to_string();
    assert!(err.contains("cleanup.interval_secs"));
}

#[test]
fn test_max_payload_bytes_default_is_none() {
    let config = JobsConfig::default();
    assert!(config.max_payload_bytes.is_none());
}
