use modo_db::config::{DatabaseConfig, JournalMode, SynchronousMode};

#[test]
fn sqlite_sub_config_deserialization() {
    let yaml = r#"
sqlite:
    path: "data/test.db"
    pragmas:
        busy_timeout: 3000
        cache_size: -8000
        journal_mode: DELETE
        synchronous: FULL
"#;
    let config: DatabaseConfig = serde_yaml_ng::from_str(yaml).unwrap();
    let sqlite = config.sqlite.unwrap();
    assert_eq!(sqlite.path, "data/test.db");
    assert_eq!(sqlite.pragmas.busy_timeout, 3000);
    assert_eq!(sqlite.pragmas.cache_size, -8000);
    assert!(matches!(sqlite.pragmas.journal_mode, JournalMode::Delete));
    assert!(matches!(sqlite.pragmas.synchronous, SynchronousMode::Full));
}

#[test]
fn postgres_sub_config_deserialization() {
    let yaml = r#"
postgres:
    url: "postgres://localhost/test"
"#;
    let config: DatabaseConfig = serde_yaml_ng::from_str(yaml).unwrap();
    let pg = config.postgres.unwrap();
    assert_eq!(pg.url, "postgres://localhost/test");
    assert!(config.sqlite.is_none());
}

#[test]
fn default_config_has_sqlite() {
    let config = DatabaseConfig::default();
    assert!(config.sqlite.is_some());
    assert!(config.postgres.is_none());
    let sqlite = config.sqlite.unwrap();
    assert_eq!(sqlite.path, "data/main.db");
    assert_eq!(sqlite.pragmas.busy_timeout, 5000);
    assert!(matches!(sqlite.pragmas.journal_mode, JournalMode::Wal));
}
