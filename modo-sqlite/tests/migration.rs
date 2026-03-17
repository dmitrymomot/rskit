use modo_sqlite::SqliteConfig;

#[tokio::test]
async fn run_migrations_creates_table() {
    let config = SqliteConfig {
        path: ":memory:".to_string(),
        ..Default::default()
    };
    let pool = modo_sqlite::connect(&config).await.unwrap();
    modo_sqlite::run_migrations(&pool).await.unwrap();

    let row: (i32,) =
        sqlx::query_as("SELECT count(*) FROM sqlite_master WHERE name = '_modo_sqlite_migrations'")
            .fetch_one(pool.pool())
            .await
            .unwrap();
    assert_eq!(row.0, 1);
}

#[tokio::test]
async fn run_migrations_is_idempotent() {
    let config = SqliteConfig {
        path: ":memory:".to_string(),
        ..Default::default()
    };
    let pool = modo_sqlite::connect(&config).await.unwrap();
    modo_sqlite::run_migrations(&pool).await.unwrap();
    modo_sqlite::run_migrations(&pool).await.unwrap(); // second call should be fine
}
