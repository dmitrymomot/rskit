use modo_sqlite::SqliteConfig;

#[tokio::test]
async fn connect_in_memory() {
    let config = SqliteConfig {
        path: ":memory:".to_string(),
        ..Default::default()
    };
    let pool = modo_sqlite::connect(&config).await.unwrap();
    let row: (i32,) = sqlx::query_as("SELECT 1")
        .fetch_one(pool.pool())
        .await
        .unwrap();
    assert_eq!(row.0, 1);
}

#[tokio::test]
async fn connect_pragmas_applied() {
    let config = SqliteConfig {
        path: ":memory:".to_string(),
        busy_timeout: 7777,
        ..Default::default()
    };
    let pool = modo_sqlite::connect(&config).await.unwrap();
    let row: (i32,) = sqlx::query_as("PRAGMA busy_timeout")
        .fetch_one(pool.pool())
        .await
        .unwrap();
    assert_eq!(row.0, 7777);
}

#[tokio::test]
async fn connect_rw_returns_two_pools() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.db");
    let config = SqliteConfig {
        path: path.to_string_lossy().to_string(),
        ..Default::default()
    };
    let (reader, writer) = modo_sqlite::connect_rw(&config).await.unwrap();

    // Write through writer
    sqlx::query("CREATE TABLE t (id INTEGER PRIMARY KEY)")
        .execute(writer.pool())
        .await
        .unwrap();

    // Read through reader
    let row: (i32,) = sqlx::query_as("SELECT count(*) FROM t")
        .fetch_one(reader.pool())
        .await
        .unwrap();
    assert_eq!(row.0, 0);
}

#[tokio::test]
async fn connect_rw_rejects_memory() {
    let config = SqliteConfig {
        path: ":memory:".to_string(),
        ..Default::default()
    };
    let result = modo_sqlite::connect_rw(&config).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn connect_rw_different_pragmas() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.db");
    let mut config = SqliteConfig {
        path: path.to_string_lossy().to_string(),
        busy_timeout: 5000,
        ..Default::default()
    };
    config.reader.busy_timeout = Some(1111);
    config.writer.busy_timeout = Some(2222);

    let (reader, writer) = modo_sqlite::connect_rw(&config).await.unwrap();

    let r: (i32,) = sqlx::query_as("PRAGMA busy_timeout")
        .fetch_one(reader.pool())
        .await
        .unwrap();
    assert_eq!(r.0, 1111);

    let w: (i32,) = sqlx::query_as("PRAGMA busy_timeout")
        .fetch_one(writer.pool())
        .await
        .unwrap();
    assert_eq!(w.0, 2222);
}
