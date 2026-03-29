#![cfg(feature = "test-helpers")]

use modo::testing::TestDb;

#[tokio::test]
async fn test_db_creates_database() {
    let db = TestDb::new().await;
    let _ = db.db();
}

#[tokio::test]
async fn test_db_exec_and_query() {
    let db = TestDb::new()
        .await
        .exec("CREATE TABLE items (id TEXT PRIMARY KEY, name TEXT NOT NULL)")
        .await
        .exec("INSERT INTO items (id, name) VALUES ('1', 'hello')")
        .await;

    use modo::db::ConnQueryExt;
    let count: i64 = db
        .db()
        .conn()
        .query_one_map("SELECT COUNT(*) FROM items", (), |row| {
            use modo::db::FromValue;
            let val = row.get_value(0).map_err(modo::Error::from)?;
            i64::from_value(val)
        })
        .await
        .unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn test_db_exec_chaining() {
    let db = TestDb::new()
        .await
        .exec("CREATE TABLE t1 (id INTEGER PRIMARY KEY)")
        .await
        .exec("CREATE TABLE t2 (id INTEGER PRIMARY KEY)")
        .await;

    use modo::db::ConnExt;
    db.db()
        .conn()
        .execute_raw("INSERT INTO t1 (id) VALUES (1)", ())
        .await
        .unwrap();
    db.db()
        .conn()
        .execute_raw("INSERT INTO t2 (id) VALUES (2)", ())
        .await
        .unwrap();
}

#[tokio::test]
async fn test_db_migrate() {
    let db = TestDb::new()
        .await
        .migrate("tests/fixtures/db_migrations")
        .await;
    let _ = db.db();
}

#[tokio::test]
#[should_panic]
async fn test_db_exec_invalid_sql_panics() {
    TestDb::new().await.exec("NOT VALID SQL").await;
}
