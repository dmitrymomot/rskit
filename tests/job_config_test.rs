#![cfg(feature = "job")]

use modo::job::JobConfig;

#[test]
fn default_config_has_sensible_values() {
    let config = JobConfig::default();
    assert_eq!(config.poll_interval_secs, 1);
    assert_eq!(config.stale_threshold_secs, 600);
    assert_eq!(config.stale_reaper_interval_secs, 60);
    assert_eq!(config.drain_timeout_secs, 30);
    assert_eq!(config.queues.len(), 1);
    assert_eq!(config.queues[0].name, "default");
    assert_eq!(config.queues[0].concurrency, 4);
    let cleanup = config.cleanup.as_ref().unwrap();
    assert_eq!(cleanup.interval_secs, 3600);
    assert_eq!(cleanup.retention_secs, 259_200);
}

#[test]
fn deserializes_from_yaml() {
    let yaml = r#"
poll_interval_secs: 2
stale_threshold_secs: 300
queues:
  - name: default
    concurrency: 8
  - name: email
    concurrency: 2
cleanup:
  interval_secs: 1800
  retention_secs: 86400
"#;
    let config: JobConfig = serde_yaml_ng::from_str(yaml).unwrap();
    assert_eq!(config.poll_interval_secs, 2);
    assert_eq!(config.queues.len(), 2);
    assert_eq!(config.queues[1].name, "email");
    assert_eq!(config.cleanup.as_ref().unwrap().retention_secs, 86400);
}

#[test]
fn cleanup_null_disables_cleanup() {
    let yaml = r#"
cleanup: null
"#;
    let config: JobConfig = serde_yaml_ng::from_str(yaml).unwrap();
    assert!(config.cleanup.is_none());
}
