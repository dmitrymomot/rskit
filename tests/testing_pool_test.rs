#![cfg(feature = "test-helpers")]

use modo::db::{ConnExt, ConnQueryExt, FromRow};
use modo::testing::TestPool;

#[derive(Debug)]
struct Item {
    id: String,
    name: String,
}

impl FromRow for Item {
    fn from_row(row: &libsql::Row) -> modo::Result<Self> {
        Ok(Self {
            id: row.get::<String>(0).map_err(modo::Error::from)?,
            name: row.get::<String>(1).map_err(modo::Error::from)?,
        })
    }
}

#[tokio::test]
async fn test_pool_default_works() {
    let pool = TestPool::new().await;
    let db = pool.conn(None).await.unwrap();
    db.conn()
        .execute_raw(
            "CREATE TABLE items (id TEXT PRIMARY KEY, name TEXT NOT NULL)",
            (),
        )
        .await
        .unwrap();
    db.conn()
        .execute_raw("INSERT INTO items (id, name) VALUES ('i1', 'Widget')", ())
        .await
        .unwrap();
    let item: Item = db
        .conn()
        .query_one(
            "SELECT id, name FROM items WHERE id = ?1",
            libsql::params!["i1"],
        )
        .await
        .unwrap();
    assert_eq!(item.id, "i1");
    assert_eq!(item.name, "Widget");
}

#[tokio::test]
async fn test_pool_shard_is_independent() {
    let pool = TestPool::new().await;

    // Create table in default
    pool.conn(None)
        .await
        .unwrap()
        .conn()
        .execute_raw("CREATE TABLE t (id TEXT PRIMARY KEY)", ())
        .await
        .unwrap();

    // Shard should NOT have it (independent in-memory DB)
    let shard = pool.conn(Some("tenant_a")).await.unwrap();
    let err = shard
        .conn()
        .execute_raw("INSERT INTO t (id) VALUES ('x')", ())
        .await;
    assert!(err.is_err());
}

#[tokio::test]
async fn test_pool_shard_is_cached() {
    let pool = TestPool::new().await;

    // First access creates, second reuses
    let shard1 = pool.conn(Some("tenant_b")).await.unwrap();
    shard1
        .conn()
        .execute_raw("CREATE TABLE cached (id TEXT PRIMARY KEY)", ())
        .await
        .unwrap();

    let shard2 = pool.conn(Some("tenant_b")).await.unwrap();
    shard2
        .conn()
        .execute_raw("INSERT INTO cached (id) VALUES ('yes')", ())
        .await
        .unwrap();
}

#[tokio::test]
async fn test_pool_exec_chaining() {
    let pool = TestPool::new()
        .await
        .exec(None, "CREATE TABLE chained (id TEXT PRIMARY KEY)")
        .await
        .exec(
            Some("shard_x"),
            "CREATE TABLE chained (id TEXT PRIMARY KEY)",
        )
        .await;

    pool.conn(None)
        .await
        .unwrap()
        .conn()
        .execute_raw("INSERT INTO chained (id) VALUES ('a')", ())
        .await
        .unwrap();

    pool.conn(Some("shard_x"))
        .await
        .unwrap()
        .conn()
        .execute_raw("INSERT INTO chained (id) VALUES ('b')", ())
        .await
        .unwrap();
}
