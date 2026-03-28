#![cfg(feature = "ldb")]

use modo::ldb;

#[tokio::test]
async fn connect_in_memory() {
    let config = ldb::Config {
        path: ":memory:".to_string(),
        ..Default::default()
    };
    let db = ldb::connect(&config).await.unwrap();
    let conn = db.conn();

    // Verify PRAGMAs were applied
    let mut rows = conn.query("PRAGMA journal_mode", ()).await.unwrap();
    let row = rows.next().await.unwrap().unwrap();
    let mode: String = row.get(0).unwrap();
    assert_eq!(mode, "memory"); // :memory: doesn't support WAL, falls back to memory

    let mut rows = conn.query("PRAGMA foreign_keys", ()).await.unwrap();
    let row = rows.next().await.unwrap().unwrap();
    let fk: i64 = row.get(0).unwrap();
    assert_eq!(fk, 1);
}

#[tokio::test]
async fn connect_file_creates_directories() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("sub/dir/test.db");

    let config = ldb::Config {
        path: db_path.to_string_lossy().to_string(),
        ..Default::default()
    };
    let db = ldb::connect(&config).await.unwrap();

    // Verify we can use the connection
    db.conn()
        .execute("CREATE TABLE test (id TEXT PRIMARY KEY)", ())
        .await
        .unwrap();

    db.conn()
        .execute("INSERT INTO test (id) VALUES ('hello')", ())
        .await
        .unwrap();

    let mut rows = db.conn().query("SELECT id FROM test", ()).await.unwrap();
    let row = rows.next().await.unwrap().unwrap();
    let id: String = row.get(0).unwrap();
    assert_eq!(id, "hello");
}

#[tokio::test]
async fn database_is_clone() {
    let config = ldb::Config {
        path: ":memory:".to_string(),
        ..Default::default()
    };
    let db = ldb::connect(&config).await.unwrap();
    let db2 = db.clone();

    db.conn()
        .execute("CREATE TABLE test (id TEXT)", ())
        .await
        .unwrap();

    // Clone shares the same connection
    let mut rows = db2
        .conn()
        .query(
            "SELECT name FROM sqlite_master WHERE type='table' AND name='test'",
            (),
        )
        .await
        .unwrap();
    assert!(rows.next().await.unwrap().is_some());
}
