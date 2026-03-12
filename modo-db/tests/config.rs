use modo_db::DatabaseConfig;

#[test]
fn test_default_config() {
    let config = DatabaseConfig::default();
    assert_eq!(config.url, "sqlite://data/main.db?mode=rwc");
    assert_eq!(config.max_connections, 5);
    assert_eq!(config.min_connections, 1);
}

#[test]
fn test_config_deserialize() {
    let yaml = r#"
url: "postgres://localhost/myapp"
max_connections: 10
min_connections: 2
"#;
    let config: DatabaseConfig = serde_yaml_ng::from_str(yaml).unwrap();
    assert_eq!(config.url, "postgres://localhost/myapp");
    assert_eq!(config.max_connections, 10);
    assert_eq!(config.min_connections, 2);
}

#[test]
fn test_config_deserialize_defaults() {
    let yaml = r#"
url: "sqlite://test.db"
"#;
    let config: DatabaseConfig = serde_yaml_ng::from_str(yaml).unwrap();
    assert_eq!(config.url, "sqlite://test.db");
    assert_eq!(config.max_connections, 5);
    assert_eq!(config.min_connections, 1);
}
