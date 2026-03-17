use modo_sqlite::SqliteConfig;

modo_sqlite::embed_migrations!(path = "tests/migrations");
modo_sqlite::embed_migrations!(path = "tests/migrations_jobs", group = "jobs");

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
async fn run_migrations_group_only_runs_specified_group() {
    let config = SqliteConfig {
        path: ":memory:".to_string(),
        ..Default::default()
    };
    let pool = modo_sqlite::connect(&config).await.unwrap();
    modo_sqlite::run_migrations_group(&pool, "jobs")
        .await
        .unwrap();

    // Jobs table should exist
    let row: (i32,) =
        sqlx::query_as("SELECT count(*) FROM sqlite_master WHERE type='table' AND name = 'jobs'")
            .fetch_one(pool.pool())
            .await
            .unwrap();
    assert_eq!(row.0, 1);

    // test_items table should NOT exist (it's in the "default" group)
    let row: (i32,) = sqlx::query_as(
        "SELECT count(*) FROM sqlite_master WHERE type='table' AND name = 'test_items'",
    )
    .fetch_one(pool.pool())
    .await
    .unwrap();
    assert_eq!(row.0, 0);
}

#[tokio::test]
async fn run_migrations_except_excludes_specified_groups() {
    let config = SqliteConfig {
        path: ":memory:".to_string(),
        ..Default::default()
    };
    let pool = modo_sqlite::connect(&config).await.unwrap();
    modo_sqlite::run_migrations_except(&pool, &["jobs"])
        .await
        .unwrap();

    // test_items table should exist (default group not excluded)
    let row: (i32,) = sqlx::query_as(
        "SELECT count(*) FROM sqlite_master WHERE type='table' AND name = 'test_items'",
    )
    .fetch_one(pool.pool())
    .await
    .unwrap();
    assert_eq!(row.0, 1);

    // Jobs table should NOT exist (excluded)
    let row: (i32,) =
        sqlx::query_as("SELECT count(*) FROM sqlite_master WHERE type='table' AND name = 'jobs'")
            .fetch_one(pool.pool())
            .await
            .unwrap();
    assert_eq!(row.0, 0);
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
