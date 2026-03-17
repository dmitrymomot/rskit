use modo_sqlite::{JournalMode, SqliteConfig, SynchronousMode};

#[test]
fn default_config() {
    let config = SqliteConfig::default();
    assert_eq!(config.path, "data/app.db");
    assert_eq!(config.max_connections, 10);
    assert_eq!(config.min_connections, 1);
    assert_eq!(config.busy_timeout, 5000);
    assert_eq!(config.cache_size, -2000);
    assert!(matches!(config.journal_mode, JournalMode::Wal));
    assert!(matches!(config.synchronous, SynchronousMode::Normal));
    assert!(config.foreign_keys);
    assert!(config.mmap_size.is_none());
}

#[test]
fn yaml_deserialization() {
    let yaml = r#"
path: "test.db"
max_connections: 20
busy_timeout: 3000
journal_mode: DELETE
synchronous: FULL
reader:
    busy_timeout: 500
    max_connections: 50
writer:
    busy_timeout: 5000
    max_connections: 1
"#;
    let config: SqliteConfig = serde_yaml_ng::from_str(yaml).unwrap();
    assert_eq!(config.path, "test.db");
    assert_eq!(config.max_connections, 20);
    assert_eq!(config.busy_timeout, 3000);
    assert!(matches!(config.journal_mode, JournalMode::Delete));
    assert_eq!(config.reader.busy_timeout, Some(500));
    assert_eq!(config.reader.max_connections, Some(50));
    assert_eq!(config.writer.busy_timeout, Some(5000));
    assert_eq!(config.writer.max_connections, Some(1));
}

#[test]
fn pool_overrides_have_smart_defaults() {
    let config = SqliteConfig::default();
    // Reader defaults: lower busy_timeout, higher cache for read-heavy workloads
    assert_eq!(config.reader.busy_timeout, Some(1000));
    assert_eq!(config.reader.cache_size, Some(-16000));
    assert_eq!(config.reader.mmap_size, Some(268435456));
    // Writer defaults: single connection, moderate timeout
    assert_eq!(config.writer.max_connections, Some(1));
    assert_eq!(config.writer.busy_timeout, Some(2000));
    assert_eq!(config.writer.cache_size, Some(-16000));
    assert_eq!(config.writer.mmap_size, Some(268435456));
}
