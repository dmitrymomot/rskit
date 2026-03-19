#[test]
fn test_sqlite_config_defaults() {
    let config = modo::db::SqliteConfig::default();
    assert_eq!(config.path, "data/app.db");
    assert_eq!(config.max_connections, 10);
    assert_eq!(config.min_connections, 1);
    assert_eq!(config.busy_timeout, 5000);
}

#[cfg(feature = "sqlite")]
#[tokio::test]
async fn test_connect_in_memory() {
    let mut config = modo::db::SqliteConfig::default();
    config.path = ":memory:".to_string();
    let pool = modo::db::connect(&config).await.unwrap();
    let row: (i64,) = sqlx::query_as("SELECT 1").fetch_one(&*pool).await.unwrap();
    assert_eq!(row.0, 1);
}

#[cfg(feature = "sqlite")]
#[tokio::test]
async fn test_connect_rw() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let mut config = modo::db::SqliteConfig::default();
    config.path = db_path.to_str().unwrap().to_string();
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

#[cfg(feature = "sqlite")]
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

    let mut config = modo::db::SqliteConfig::default();
    config.path = db_path.to_str().unwrap().to_string();
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
