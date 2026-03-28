#![cfg(feature = "ldb")]

use modo::error::Result;
use modo::ldb;
use modo::ldb::{ColumnMap, ConnQueryExt, FromRow};

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

// -- FromRow / ColumnMap types --

struct User {
    id: String,
    name: String,
    email: String,
}

// Positional FromRow
impl FromRow for User {
    fn from_row(row: &libsql::Row) -> Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            name: row.get(1)?,
            email: row.get(2)?,
        })
    }
}

struct UserNamed {
    id: String,
    name: String,
    email: String,
}

// Name-based FromRow
impl FromRow for UserNamed {
    fn from_row(row: &libsql::Row) -> Result<Self> {
        let c = ColumnMap::from_row(row);
        Ok(Self {
            id: c.get(row, "id")?,
            name: c.get(row, "name")?,
            email: c.get(row, "email")?,
        })
    }
}

#[tokio::test]
async fn from_row_positional() {
    let db = test_db().await;
    let conn = db.conn();
    seed_users(conn).await;

    let mut rows = conn
        .query("SELECT id, name, email FROM users WHERE id = 'u1'", ())
        .await
        .unwrap();
    let row = rows.next().await.unwrap().unwrap();
    let user = User::from_row(&row).unwrap();
    assert_eq!(user.id, "u1");
    assert_eq!(user.name, "Alice");
    assert_eq!(user.email, "alice@test.com");
}

#[tokio::test]
async fn from_row_named() {
    let db = test_db().await;
    let conn = db.conn();
    seed_users(conn).await;

    // Different column order in SELECT — name-based still works
    let mut rows = conn
        .query("SELECT email, id, name FROM users WHERE id = 'u1'", ())
        .await
        .unwrap();
    let row = rows.next().await.unwrap().unwrap();
    let user = UserNamed::from_row(&row).unwrap();
    assert_eq!(user.id, "u1");
    assert_eq!(user.name, "Alice");
    assert_eq!(user.email, "alice@test.com");
}

#[tokio::test]
async fn column_map_missing_column_returns_error() {
    let db = test_db().await;
    let conn = db.conn();
    seed_users(conn).await;

    let mut rows = conn
        .query("SELECT id FROM users WHERE id = 'u1'", ())
        .await
        .unwrap();
    let row = rows.next().await.unwrap().unwrap();
    let c = ColumnMap::from_row(&row);
    let result: Result<String> = c.get(&row, "nonexistent");
    assert!(result.is_err());
}

// -- ConnExt / ConnQueryExt tests --

#[tokio::test]
async fn conn_ext_query_one() {
    let db = test_db().await;
    let conn = db.conn();
    seed_users(conn).await;

    let user: User = conn
        .query_one("SELECT id, name, email FROM users WHERE id = ?1", libsql::params!["u1"])
        .await
        .unwrap();
    assert_eq!(user.id, "u1");
    assert_eq!(user.name, "Alice");
}

#[tokio::test]
async fn conn_ext_query_one_not_found() {
    let db = test_db().await;
    let conn = db.conn();
    seed_users(conn).await;

    let result: modo::error::Result<User> = conn
        .query_one(
            "SELECT id, name, email FROM users WHERE id = ?1",
            libsql::params!["nonexistent"],
        )
        .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn conn_ext_query_optional() {
    let db = test_db().await;
    let conn = db.conn();
    seed_users(conn).await;

    let found: Option<User> = conn
        .query_optional(
            "SELECT id, name, email FROM users WHERE id = ?1",
            libsql::params!["u1"],
        )
        .await
        .unwrap();
    assert!(found.is_some());

    let missing: Option<User> = conn
        .query_optional(
            "SELECT id, name, email FROM users WHERE id = ?1",
            libsql::params!["nonexistent"],
        )
        .await
        .unwrap();
    assert!(missing.is_none());
}

#[tokio::test]
async fn conn_ext_query_all() {
    let db = test_db().await;
    let conn = db.conn();
    seed_users(conn).await;

    let users: Vec<User> = conn
        .query_all("SELECT id, name, email FROM users ORDER BY id", ())
        .await
        .unwrap();
    assert_eq!(users.len(), 2);
    assert_eq!(users[0].id, "u1");
    assert_eq!(users[1].id, "u2");
}

#[tokio::test]
async fn conn_ext_query_one_map() {
    let db = test_db().await;
    let conn = db.conn();
    seed_users(conn).await;

    let name: String = conn
        .query_one_map(
            "SELECT name FROM users WHERE id = ?1",
            libsql::params!["u1"],
            |row| Ok(row.get::<String>(0)?),
        )
        .await
        .unwrap();
    assert_eq!(name, "Alice");
}

#[tokio::test]
async fn conn_ext_works_on_transaction() {
    let db = test_db().await;
    let conn = db.conn();
    seed_users(conn).await;

    let tx = conn.transaction().await.unwrap();
    tx.execute(
        "INSERT INTO users (id, name, email) VALUES ('u3', 'Charlie', 'charlie@test.com')",
        (),
    )
    .await
    .unwrap();

    let user: User = tx
        .query_one(
            "SELECT id, name, email FROM users WHERE id = ?1",
            libsql::params!["u3"],
        )
        .await
        .unwrap();
    assert_eq!(user.name, "Charlie");

    tx.commit().await.unwrap();
}

// -- Migration tests --

#[tokio::test]
async fn migrate_applies_sql_files_in_order() {
    let db = test_db().await;
    let conn = db.conn();

    ldb::migrate(conn, "tests/fixtures/ldb_migrations")
        .await
        .unwrap();

    // Verify table was created with bio column
    conn.execute(
        "INSERT INTO users (id, name, email, bio) VALUES ('u1', 'Alice', 'a@t.com', 'hello')",
        (),
    )
    .await
    .unwrap();

    let mut rows = conn
        .query("SELECT bio FROM users WHERE id = 'u1'", ())
        .await
        .unwrap();
    let row = rows.next().await.unwrap().unwrap();
    let bio: String = row.get(0).unwrap();
    assert_eq!(bio, "hello");
}

#[tokio::test]
async fn migrate_is_idempotent() {
    let db = test_db().await;
    let conn = db.conn();

    ldb::migrate(conn, "tests/fixtures/ldb_migrations")
        .await
        .unwrap();
    // Run again — should not error
    ldb::migrate(conn, "tests/fixtures/ldb_migrations")
        .await
        .unwrap();

    // Verify _migrations table has 2 entries
    let mut rows = conn
        .query("SELECT COUNT(*) FROM _migrations", ())
        .await
        .unwrap();
    let row = rows.next().await.unwrap().unwrap();
    let count: i64 = row.get(0).unwrap();
    assert_eq!(count, 2);
}

#[tokio::test]
async fn migrate_nonexistent_dir_is_ok() {
    let db = test_db().await;
    let conn = db.conn();

    // Should not error — just skip
    ldb::migrate(conn, "tests/fixtures/nonexistent")
        .await
        .unwrap();
}

// -- Test helpers --

async fn test_db() -> ldb::Database {
    let config = ldb::Config {
        path: ":memory:".to_string(),
        ..Default::default()
    };
    ldb::connect(&config).await.unwrap()
}

async fn seed_users(conn: &libsql::Connection) {
    conn.execute(
        "CREATE TABLE users (id TEXT PRIMARY KEY, name TEXT NOT NULL, email TEXT NOT NULL)",
        (),
    )
    .await
    .unwrap();
    conn.execute(
        "INSERT INTO users (id, name, email) VALUES ('u1', 'Alice', 'alice@test.com')",
        (),
    )
    .await
    .unwrap();
    conn.execute(
        "INSERT INTO users (id, name, email) VALUES ('u2', 'Bob', 'bob@test.com')",
        (),
    )
    .await
    .unwrap();
}
