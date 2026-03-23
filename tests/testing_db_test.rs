#![cfg(feature = "test-helpers")]

use modo::testing::TestDb;

#[tokio::test]
async fn test_new_creates_pool() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let row: (i64,) = sqlx::query_as("SELECT 1").fetch_one(&*pool).await.unwrap();
    assert_eq!(row.0, 1);
}

#[tokio::test]
async fn test_exec_creates_table() {
    let db = TestDb::new()
        .await
        .exec("CREATE TABLE test_items (id TEXT PRIMARY KEY, name TEXT NOT NULL)")
        .await;

    sqlx::query("INSERT INTO test_items (id, name) VALUES ('1', 'Alice')")
        .execute(&*db.pool())
        .await
        .unwrap();

    let row: (String,) = sqlx::query_as("SELECT name FROM test_items WHERE id = '1'")
        .fetch_one(&*db.pool())
        .await
        .unwrap();
    assert_eq!(row.0, "Alice");
}

#[tokio::test]
async fn test_exec_chaining() {
    let db = TestDb::new()
        .await
        .exec("CREATE TABLE t1 (id INTEGER PRIMARY KEY)")
        .await
        .exec("CREATE TABLE t2 (id INTEGER PRIMARY KEY)")
        .await;

    sqlx::query("INSERT INTO t1 (id) VALUES (1)")
        .execute(&*db.pool())
        .await
        .unwrap();
    sqlx::query("INSERT INTO t2 (id) VALUES (2)")
        .execute(&*db.pool())
        .await
        .unwrap();
}

#[tokio::test]
async fn test_read_pool_and_write_pool_share_data() {
    let db = TestDb::new()
        .await
        .exec("CREATE TABLE shared (id TEXT PRIMARY KEY)")
        .await;

    sqlx::query("INSERT INTO shared (id) VALUES ('x')")
        .execute(&*db.write_pool())
        .await
        .unwrap();

    let row: (String,) = sqlx::query_as("SELECT id FROM shared")
        .fetch_one(&*db.read_pool())
        .await
        .unwrap();
    assert_eq!(row.0, "x");
}

#[tokio::test]
async fn test_pool_read_pool_write_pool_all_share_same_db() {
    let db = TestDb::new()
        .await
        .exec("CREATE TABLE multi (id TEXT PRIMARY KEY)")
        .await;

    sqlx::query("INSERT INTO multi (id) VALUES ('a')")
        .execute(&*db.pool())
        .await
        .unwrap();

    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM multi")
        .fetch_one(&*db.read_pool())
        .await
        .unwrap();
    assert_eq!(count.0, 1);

    sqlx::query("INSERT INTO multi (id) VALUES ('b')")
        .execute(&*db.write_pool())
        .await
        .unwrap();

    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM multi")
        .fetch_one(&*db.pool())
        .await
        .unwrap();
    assert_eq!(count.0, 2);
}

#[tokio::test]
#[should_panic]
async fn test_exec_panics_on_invalid_sql() {
    TestDb::new().await.exec("NOT VALID SQL").await;
}
