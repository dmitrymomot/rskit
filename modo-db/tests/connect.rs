use modo_db::DatabaseConfig;

#[tokio::test]
async fn test_connect_sqlite_in_memory() {
    let config = DatabaseConfig {
        url: "sqlite::memory:".to_string(),
        ..Default::default()
    };
    let db = modo_db::connect(&config).await.unwrap();
    use sea_orm::ConnectionTrait;
    db.execute_unprepared("SELECT 1").await.unwrap();
}

#[tokio::test]
async fn test_sync_and_migrate_empty() {
    let config = DatabaseConfig {
        url: "sqlite::memory:".to_string(),
        ..Default::default()
    };
    let db = modo_db::connect(&config).await.unwrap();
    modo_db::sync_and_migrate(&db).await.unwrap();

    use sea_orm::ConnectionTrait;
    db.execute_unprepared("SELECT * FROM _modo_migrations")
        .await
        .unwrap();
}
