#![cfg(feature = "db")]

use modo::db::{self, ConnExt};

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
  lock_shards: 8
"#;
    let config: db::Config = serde_yaml_ng::from_str(yaml).unwrap();
    let pool = config.pool.unwrap();
    assert_eq!(pool.base_path, "data/shards");
    assert_eq!(pool.lock_shards, 8);
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
    assert_eq!(pool.lock_shards, 16);
}

#[tokio::test]
async fn pool_new_fails_without_pool_config() {
    let config = db::Config::default(); // pool is None
    let result = db::DatabasePool::new(&config).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn pool_conn_none_returns_default() {
    let config = db::Config {
        path: ":memory:".to_string(),
        pool: Some(db::PoolConfig {
            base_path: "data/test_shards".to_string(),
            lock_shards: 4,
        }),
        ..Default::default()
    };
    let pool = db::DatabasePool::new(&config).await.unwrap();

    // conn(None) returns the default database — verify it works
    let db = pool.conn(None).await.unwrap();
    let result: u64 = db
        .conn()
        .execute_raw("CREATE TABLE test_default (id TEXT PRIMARY KEY)", ())
        .await
        .unwrap();
    assert_eq!(result, 0);
}

#[tokio::test]
async fn pool_conn_shard_opens_new_database() {
    let dir = tempfile::tempdir().unwrap();
    let config = db::Config {
        path: ":memory:".to_string(),
        pool: Some(db::PoolConfig {
            base_path: dir.path().to_str().unwrap().to_string(),
            lock_shards: 4,
        }),
        ..Default::default()
    };
    let pool = db::DatabasePool::new(&config).await.unwrap();

    // First call to a shard creates the database
    let shard_db = pool.conn(Some("tenant_abc")).await.unwrap();
    shard_db
        .conn()
        .execute_raw("CREATE TABLE shard_test (id TEXT PRIMARY KEY)", ())
        .await
        .unwrap();

    // Second call returns the cached connection — table should exist
    let shard_db2 = pool.conn(Some("tenant_abc")).await.unwrap();
    shard_db2
        .conn()
        .execute_raw("INSERT INTO shard_test (id) VALUES ('hello')", ())
        .await
        .unwrap();
}

#[tokio::test]
async fn pool_shards_are_independent() {
    let dir = tempfile::tempdir().unwrap();
    let config = db::Config {
        path: ":memory:".to_string(),
        pool: Some(db::PoolConfig {
            base_path: dir.path().to_str().unwrap().to_string(),
            lock_shards: 4,
        }),
        ..Default::default()
    };
    let pool = db::DatabasePool::new(&config).await.unwrap();

    // Create a table in shard A
    let shard_a = pool.conn(Some("shard_a")).await.unwrap();
    shard_a
        .conn()
        .execute_raw("CREATE TABLE only_in_a (id TEXT PRIMARY KEY)", ())
        .await
        .unwrap();

    // Shard B should NOT have that table
    let shard_b = pool.conn(Some("shard_b")).await.unwrap();
    let err = shard_b
        .conn()
        .execute_raw("INSERT INTO only_in_a (id) VALUES ('x')", ())
        .await;
    assert!(err.is_err());
}

#[tokio::test]
async fn pool_is_clone() {
    let config = db::Config {
        path: ":memory:".to_string(),
        pool: Some(db::PoolConfig {
            base_path: "data/test_shards".to_string(),
            lock_shards: 4,
        }),
        ..Default::default()
    };
    let pool = db::DatabasePool::new(&config).await.unwrap();
    let pool2 = pool.clone();

    // Both clones access the same default database
    pool.conn(None)
        .await
        .unwrap()
        .conn()
        .execute_raw("CREATE TABLE clone_test (id TEXT PRIMARY KEY)", ())
        .await
        .unwrap();

    pool2
        .conn(None)
        .await
        .unwrap()
        .conn()
        .execute_raw("INSERT INTO clone_test (id) VALUES ('from_clone2')", ())
        .await
        .unwrap();
}

#[tokio::test]
async fn pool_shard_runs_migrations() {
    let dir = tempfile::tempdir().unwrap();
    let migrations_dir = dir.path().join("migrations");
    std::fs::create_dir_all(&migrations_dir).unwrap();
    std::fs::write(
        migrations_dir.join("001_create_users.sql"),
        "CREATE TABLE IF NOT EXISTS users (id TEXT PRIMARY KEY, name TEXT NOT NULL);",
    )
    .unwrap();

    let config = db::Config {
        path: dir.path().join("main.db").to_str().unwrap().to_string(),
        migrations: Some(migrations_dir.to_str().unwrap().to_string()),
        pool: Some(db::PoolConfig {
            base_path: dir.path().join("shards").to_str().unwrap().to_string(),
            lock_shards: 4,
        }),
        ..Default::default()
    };
    let pool = db::DatabasePool::new(&config).await.unwrap();

    // Shard should have the users table from migrations
    let shard = pool.conn(Some("tenant_xyz")).await.unwrap();
    shard
        .conn()
        .execute_raw("INSERT INTO users (id, name) VALUES ('u1', 'Alice')", ())
        .await
        .unwrap();
}

#[tokio::test]
async fn managed_pool_can_shutdown() {
    let config = db::Config {
        path: ":memory:".to_string(),
        pool: Some(db::PoolConfig {
            base_path: "data/test_shards".to_string(),
            lock_shards: 4,
        }),
        ..Default::default()
    };
    let pool = db::DatabasePool::new(&config).await.unwrap();
    let managed = db::managed_pool(pool);
    // Verify it implements Task by calling shutdown
    use modo::runtime::Task;
    managed.shutdown().await.unwrap();
}

#[tokio::test]
async fn pool_rejects_zero_lock_shards() {
    let config = db::Config {
        path: ":memory:".to_string(),
        pool: Some(db::PoolConfig {
            base_path: ":memory:".to_string(),
            lock_shards: 0,
        }),
        ..Default::default()
    };
    assert!(db::DatabasePool::new(&config).await.is_err());
}

#[tokio::test]
async fn pool_rejects_invalid_shard_names() {
    let config = db::Config {
        path: ":memory:".to_string(),
        pool: Some(db::PoolConfig {
            base_path: ":memory:".to_string(),
            lock_shards: 4,
        }),
        ..Default::default()
    };
    let pool = db::DatabasePool::new(&config).await.unwrap();

    assert!(pool.conn(Some("")).await.is_err());
    assert!(pool.conn(Some("../escape")).await.is_err());
    assert!(pool.conn(Some("back\\slash")).await.is_err());
    assert!(pool.conn(Some(".hidden")).await.is_err());
    assert!(pool.conn(Some("null\0byte")).await.is_err());
    assert!(pool.conn(Some("valid_name")).await.is_ok());
}
