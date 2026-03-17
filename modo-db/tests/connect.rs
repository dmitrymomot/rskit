use modo_db::config::{DatabaseConfig, SqliteConfig, SqliteDbConfig};

#[tokio::test]
async fn test_connect_sqlite_in_memory() {
    let config = DatabaseConfig {
        sqlite: Some(SqliteDbConfig {
            path: ":memory:".to_string(),
            ..Default::default()
        }),
        ..Default::default()
    };
    let db = modo_db::connect(&config).await.unwrap();
    use sea_orm::ConnectionTrait;
    db.execute_unprepared("SELECT 1").await.unwrap();
}

#[tokio::test]
async fn test_pragmas_applied_on_all_connections() {
    let config = DatabaseConfig {
        max_connections: 3,
        min_connections: 3,
        sqlite: Some(SqliteDbConfig {
            path: ":memory:".to_string(),
            pragmas: SqliteConfig {
                busy_timeout: 7777,
                ..Default::default()
            },
            ..Default::default()
        }),
        ..Default::default()
    };
    let db = modo_db::connect(&config).await.unwrap();

    // Query PRAGMA on multiple connections by running concurrent queries.
    // Each should return the configured value, not the default.
    use sea_orm::ConnectionTrait;
    for _ in 0..3 {
        let result = db
            .query_one_raw(sea_orm::Statement::from_string(
                sea_orm::DatabaseBackend::Sqlite,
                "PRAGMA busy_timeout".to_string(),
            ))
            .await
            .unwrap()
            .unwrap();
        let timeout: i32 = result.try_get_by_index(0).unwrap();
        assert_eq!(timeout, 7777);
    }
}

#[tokio::test]
async fn test_sync_and_migrate_empty() {
    let config = DatabaseConfig {
        sqlite: Some(SqliteDbConfig {
            path: ":memory:".to_string(),
            ..Default::default()
        }),
        ..Default::default()
    };
    let db = modo_db::connect(&config).await.unwrap();
    modo_db::sync_and_migrate(&db).await.unwrap();

    use sea_orm::ConnectionTrait;
    db.execute_unprepared("SELECT * FROM _modo_migrations")
        .await
        .unwrap();
}

#[tokio::test]
async fn test_sync_and_migrate_group() {
    let config = DatabaseConfig {
        sqlite: Some(SqliteDbConfig {
            path: ":memory:".to_string(),
            ..Default::default()
        }),
        ..Default::default()
    };
    let db = modo_db::connect(&config).await.unwrap();
    modo_db::sync_and_migrate_group(&db, "nonexistent")
        .await
        .unwrap();

    use sea_orm::ConnectionTrait;
    db.execute_unprepared("SELECT * FROM _modo_migrations")
        .await
        .unwrap();
}
