use modo_sqlite::SqliteConfig;

modo_sqlite::embed_migrations!(path = "tests/migrations");

#[tokio::test]
async fn embed_and_run_migrations() {
    let config = modo_sqlite::SqliteConfig {
        path: ":memory:".to_string(),
        ..Default::default()
    };
    let pool = modo_sqlite::connect(&config).await.unwrap();
    modo_sqlite::run_migrations(&pool).await.unwrap();

    // Table should exist with the added column
    sqlx::query("INSERT INTO test_items (id, name, status) VALUES ('1', 'test', 'done')")
        .execute(pool.pool())
        .await
        .unwrap();

    let row: (String, String, String) =
        sqlx::query_as("SELECT id, name, status FROM test_items WHERE id = '1'")
            .fetch_one(pool.pool())
            .await
            .unwrap();
    assert_eq!(row.0, "1");
    assert_eq!(row.1, "test");
    assert_eq!(row.2, "done");
}

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
