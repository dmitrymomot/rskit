#[test]
fn test_sqlite_config_defaults() {
    let config = modo::db::SqliteConfig::default();
    assert_eq!(config.path, "data/app.db");
    assert_eq!(config.max_connections, 10);
    assert_eq!(config.min_connections, 1);
    assert_eq!(config.busy_timeout, 5000);
}

#[tokio::test]
async fn test_connect_in_memory() {
    let config = modo::db::SqliteConfig {
        path: ":memory:".to_string(),
        ..Default::default()
    };
    let pool = modo::db::connect(&config).await.unwrap();
    let row: (i64,) = sqlx::query_as("SELECT 1").fetch_one(&*pool).await.unwrap();
    assert_eq!(row.0, 1);
}

#[tokio::test]
async fn test_connect_rw() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let config = modo::db::SqliteConfig {
        path: db_path.to_str().unwrap().to_string(),
        ..Default::default()
    };
    let (reader, writer) = modo::db::connect_rw(&config).await.unwrap();

    sqlx::query("CREATE TABLE test (id INTEGER PRIMARY KEY)")
        .execute(&*writer)
        .await
        .unwrap();

    let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM test")
        .fetch_one(&*reader)
        .await
        .unwrap();
    assert_eq!(row.0, 0);
}

#[tokio::test]
async fn test_migrate_from_directory() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("migrate_test.db");
    let migrations_dir = dir.path().join("migrations");
    std::fs::create_dir_all(&migrations_dir).unwrap();

    std::fs::write(
        migrations_dir.join("001_create_users.sql"),
        "CREATE TABLE users (id TEXT PRIMARY KEY, name TEXT NOT NULL);",
    )
    .unwrap();

    let config = modo::db::SqliteConfig {
        path: db_path.to_str().unwrap().to_string(),
        ..Default::default()
    };
    let pool = modo::db::connect(&config).await.unwrap();
    modo::db::migrate(migrations_dir.to_str().unwrap(), &pool)
        .await
        .unwrap();

    let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
        .fetch_one(&*pool)
        .await
        .unwrap();
    assert_eq!(row.0, 0);
}

#[tokio::test]
async fn test_connect_rw_rejects_memory() {
    let config = modo::db::SqliteConfig {
        path: ":memory:".to_string(),
        ..Default::default()
    };
    let result = modo::db::connect_rw(&config).await;
    assert!(result.is_err());
    let err = result.err().unwrap();
    assert!(
        err.message().contains("in-memory"),
        "expected in-memory rejection error, got: {}",
        err.message()
    );
}

#[tokio::test]
async fn test_managed_pool_shutdown() {
    let config = modo::db::SqliteConfig {
        path: ":memory:".to_string(),
        ..Default::default()
    };
    let pool = modo::db::connect(&config).await.unwrap();

    let managed = modo::db::managed(pool.clone());
    let row: (i64,) = sqlx::query_as("SELECT 1").fetch_one(&*pool).await.unwrap();
    assert_eq!(row.0, 1);

    use modo::runtime::Task;
    managed.shutdown().await.unwrap();
    assert!(pool.is_closed());
}

// --- New tests: config enums and overrides ---

#[test]
fn test_config_enum_display() {
    use modo::db::{JournalMode, SynchronousMode, TempStore};

    assert_eq!(format!("{}", JournalMode::Delete), "DELETE");
    assert_eq!(format!("{}", JournalMode::Truncate), "TRUNCATE");
    assert_eq!(format!("{}", JournalMode::Persist), "PERSIST");
    assert_eq!(format!("{}", JournalMode::Memory), "MEMORY");
    assert_eq!(format!("{}", JournalMode::Wal), "WAL");
    assert_eq!(format!("{}", JournalMode::Off), "OFF");

    assert_eq!(format!("{}", SynchronousMode::Off), "OFF");
    assert_eq!(format!("{}", SynchronousMode::Normal), "NORMAL");
    assert_eq!(format!("{}", SynchronousMode::Full), "FULL");
    assert_eq!(format!("{}", SynchronousMode::Extra), "EXTRA");

    assert_eq!(format!("{}", TempStore::Default), "DEFAULT");
    assert_eq!(format!("{}", TempStore::File), "FILE");
    assert_eq!(format!("{}", TempStore::Memory), "MEMORY");
}

#[test]
fn test_pool_overrides_defaults() {
    use modo::db::PoolOverrides;

    let reader = PoolOverrides::default_reader();
    assert_eq!(reader.busy_timeout, Some(1000));
    assert_eq!(reader.cache_size, Some(-16000));
    assert_eq!(reader.mmap_size, Some(268_435_456));
    assert_eq!(reader.max_connections, None);

    let writer = PoolOverrides::default_writer();
    assert_eq!(writer.max_connections, Some(1));
    assert_eq!(writer.busy_timeout, Some(2000));
    assert_eq!(writer.cache_size, Some(-16000));
    assert_eq!(writer.mmap_size, Some(268_435_456));
}

// --- New tests: pool traits ---

#[tokio::test]
async fn test_pool_reader_writer_traits() {
    use modo::db::{Reader, Writer};

    let config = modo::db::SqliteConfig {
        path: ":memory:".to_string(),
        ..Default::default()
    };
    let pool = modo::db::connect(&config).await.unwrap();

    // Pool implements Reader and Writer
    let read_inner = pool.read_pool();
    let row: (i64,) = sqlx::query_as("SELECT 1")
        .fetch_one(read_inner)
        .await
        .unwrap();
    assert_eq!(row.0, 1);

    let write_inner = pool.write_pool();
    let row: (i64,) = sqlx::query_as("SELECT 1")
        .fetch_one(write_inner)
        .await
        .unwrap();
    assert_eq!(row.0, 1);
}

#[tokio::test]
async fn test_managed_from_read_and_write_pools() {
    use modo::runtime::Task;

    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("managed_rw.db");
    let config = modo::db::SqliteConfig {
        path: db_path.to_str().unwrap().to_string(),
        ..Default::default()
    };
    let (reader, writer) = modo::db::connect_rw(&config).await.unwrap();

    // WritePool → ManagedPool
    let managed_writer = modo::db::managed(writer);
    managed_writer.shutdown().await.unwrap();

    // ReadPool → ManagedPool
    let managed_reader = modo::db::managed(reader);
    managed_reader.shutdown().await.unwrap();
}

// --- New tests: PRAGMA verification ---

#[tokio::test]
async fn test_pragma_settings_applied() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("pragma_test.db");
    let config = modo::db::SqliteConfig {
        path: db_path.to_str().unwrap().to_string(),
        ..Default::default()
    };
    let pool = modo::db::connect(&config).await.unwrap();

    let (journal_mode,): (String,) = sqlx::query_as("PRAGMA journal_mode")
        .fetch_one(&*pool)
        .await
        .unwrap();
    assert_eq!(journal_mode, "wal");

    let (foreign_keys,): (i64,) = sqlx::query_as("PRAGMA foreign_keys")
        .fetch_one(&*pool)
        .await
        .unwrap();
    assert_eq!(foreign_keys, 1);

    let (busy_timeout,): (i64,) = sqlx::query_as("PRAGMA busy_timeout")
        .fetch_one(&*pool)
        .await
        .unwrap();
    assert_eq!(busy_timeout, 5000);

    let (synchronous,): (i64,) = sqlx::query_as("PRAGMA synchronous")
        .fetch_one(&*pool)
        .await
        .unwrap();
    assert_eq!(synchronous, 1); // NORMAL = 1

    let (cache_size,): (i64,) = sqlx::query_as("PRAGMA cache_size")
        .fetch_one(&*pool)
        .await
        .unwrap();
    assert_eq!(cache_size, -2000);
}

// --- New tests: sqlx error conversions ---

#[tokio::test]
async fn test_sqlx_row_not_found_maps_to_404() {
    let config = modo::db::SqliteConfig {
        path: ":memory:".to_string(),
        ..Default::default()
    };
    let pool = modo::db::connect(&config).await.unwrap();

    sqlx::query("CREATE TABLE test_empty (id INTEGER PRIMARY KEY)")
        .execute(&*pool)
        .await
        .unwrap();

    let sqlx_err = sqlx::query_as::<_, (i64,)>("SELECT id FROM test_empty")
        .fetch_one(&*pool)
        .await
        .expect_err("should fail with RowNotFound");

    let err: modo::Error = sqlx_err.into();
    assert_eq!(err.status().as_u16(), 404);
    assert_eq!(err.message(), "record not found");
}

#[tokio::test]
async fn test_sqlx_unique_violation_maps_to_409() {
    let config = modo::db::SqliteConfig {
        path: ":memory:".to_string(),
        ..Default::default()
    };
    let pool = modo::db::connect(&config).await.unwrap();

    sqlx::query("CREATE TABLE test_unique (id INTEGER PRIMARY KEY, name TEXT UNIQUE)")
        .execute(&*pool)
        .await
        .unwrap();

    sqlx::query("INSERT INTO test_unique (id, name) VALUES (1, 'alice')")
        .execute(&*pool)
        .await
        .unwrap();

    let sqlx_err = sqlx::query("INSERT INTO test_unique (id, name) VALUES (2, 'alice')")
        .execute(&*pool)
        .await
        .expect_err("should fail with unique violation");

    let err: modo::Error = sqlx_err.into();
    assert_eq!(err.status().as_u16(), 409);
    assert_eq!(err.message(), "record already exists");
}

#[tokio::test]
async fn test_sqlx_fk_violation_maps_to_400() {
    let config = modo::db::SqliteConfig {
        path: ":memory:".to_string(),
        ..Default::default()
    };
    let pool = modo::db::connect(&config).await.unwrap();

    sqlx::query("CREATE TABLE parent (id INTEGER PRIMARY KEY)")
        .execute(&*pool)
        .await
        .unwrap();

    sqlx::query(
        "CREATE TABLE child (id INTEGER PRIMARY KEY, parent_id INTEGER REFERENCES parent(id))",
    )
    .execute(&*pool)
    .await
    .unwrap();

    let sqlx_err = sqlx::query("INSERT INTO child (id, parent_id) VALUES (1, 999)")
        .execute(&*pool)
        .await
        .expect_err("should fail with FK violation");

    let err: modo::Error = sqlx_err.into();
    assert_eq!(err.status().as_u16(), 400);
    assert_eq!(err.message(), "foreign key violation");
}
