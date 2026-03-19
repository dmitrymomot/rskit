#[test]
fn test_sqlite_config_defaults() {
    let config = modo::db::SqliteConfig::default();
    assert_eq!(config.path, "data/app.db");
    assert_eq!(config.max_connections, 10);
    assert_eq!(config.min_connections, 1);
    assert_eq!(config.busy_timeout, 5000);
}
