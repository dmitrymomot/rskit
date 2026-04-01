#![cfg(feature = "db")]

use modo::db;

#[test]
fn config_pool_defaults_to_none() {
    let config = db::Config::default();
    assert!(config.pool.is_none());
}

#[test]
fn config_pool_deserializes_from_yaml() {
    let yaml = r#"
path: "data/app.db"
pool:
  base_path: "data/shards"
  shard_count: 8
"#;
    let config: db::Config = serde_yaml_ng::from_str(yaml).unwrap();
    let pool = config.pool.unwrap();
    assert_eq!(pool.base_path, "data/shards");
    assert_eq!(pool.shard_count, 8);
}

#[test]
fn pool_config_defaults() {
    let yaml = r#"
path: "data/app.db"
pool: {}
"#;
    let config: db::Config = serde_yaml_ng::from_str(yaml).unwrap();
    let pool = config.pool.unwrap();
    assert_eq!(pool.base_path, "data/shards");
    assert_eq!(pool.shard_count, 16);
}
