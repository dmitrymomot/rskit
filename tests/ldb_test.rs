#![cfg(feature = "ldb")]

use std::collections::HashMap;

use modo::error::Result;
use modo::ldb;
use modo::ldb::{
    ColumnMap, ConnExt, ConnQueryExt, FieldType, Filter, FilterSchema, FromRow, PageRequest,
};

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
        .query_one(
            "SELECT id, name, email FROM users WHERE id = ?1",
            libsql::params!["u1"],
        )
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

// -- Managed shutdown --

#[tokio::test]
async fn managed_shutdown() {
    let db = test_db().await;
    let managed = ldb::managed(db);
    // ManagedDatabase implements Task — shutdown should succeed
    use modo::runtime::Task;
    managed.shutdown().await.unwrap();
}

// -- Pagination --

#[tokio::test]
async fn page_request_clamp() {
    let config = ldb::PaginationConfig {
        default_per_page: 20,
        max_per_page: 50,
    };

    let mut req = ldb::PageRequest {
        page: 0,
        per_page: 0,
    };
    req.clamp(&config);
    assert_eq!(req.page, 1);
    assert_eq!(req.per_page, 20);

    let mut req = ldb::PageRequest {
        page: 3,
        per_page: 200,
    };
    req.clamp(&config);
    assert_eq!(req.page, 3);
    assert_eq!(req.per_page, 50);
}

#[tokio::test]
async fn page_new_calculates_fields() {
    let page: ldb::Page<String> = ldb::Page::new(vec!["a".into(), "b".into()], 5, 2, 2);
    assert_eq!(page.total_pages, 3);
    assert!(page.has_next);
    assert!(page.has_prev);
}

#[tokio::test]
async fn cursor_request_clamp() {
    let config = ldb::PaginationConfig::default();

    let mut req = ldb::CursorRequest {
        after: None,
        per_page: 0,
    };
    req.clamp(&config);
    assert_eq!(req.per_page, 20);

    let mut req = ldb::CursorRequest {
        after: Some("abc".into()),
        per_page: 999,
    };
    req.clamp(&config);
    assert_eq!(req.per_page, 100);
}

// -- Filter DSL tests --

#[test]
fn filter_eq_single_value() {
    let schema = FilterSchema::new().field("status", FieldType::Text);
    let mut params = HashMap::new();
    params.insert("status".into(), vec!["active".into()]);

    let filter = Filter::from_query_params(&params);
    let validated = filter.validate(&schema).unwrap();
    assert_eq!(validated.clauses.len(), 1);
    assert_eq!(validated.clauses[0], "status = ?");
    assert_eq!(validated.params.len(), 1);
}

#[test]
fn filter_in_multiple_values() {
    let schema = FilterSchema::new().field("status", FieldType::Text);
    let mut params = HashMap::new();
    params.insert("status".into(), vec!["active".into(), "pending".into()]);

    let filter = Filter::from_query_params(&params);
    let validated = filter.validate(&schema).unwrap();
    assert_eq!(validated.clauses[0], "status IN (?, ?)");
    assert_eq!(validated.params.len(), 2);
}

#[test]
fn filter_operators() {
    let schema = FilterSchema::new()
        .field("age", FieldType::Int)
        .field("name", FieldType::Text);

    let mut params = HashMap::new();
    params.insert("age.gte".into(), vec!["18".into()]);
    params.insert("name.like".into(), vec!["john%".into()]);

    let filter = Filter::from_query_params(&params);
    let validated = filter.validate(&schema).unwrap();
    assert_eq!(validated.clauses.len(), 2);
    assert_eq!(validated.params.len(), 2);
}

#[test]
fn filter_null_operator() {
    let schema = FilterSchema::new().field("deleted_at", FieldType::Date);
    let mut params = HashMap::new();
    params.insert("deleted_at.null".into(), vec!["true".into()]);

    let filter = Filter::from_query_params(&params);
    let validated = filter.validate(&schema).unwrap();
    assert_eq!(validated.clauses[0], "deleted_at IS NULL");
    assert_eq!(validated.params.len(), 0);
}

#[test]
fn filter_unknown_columns_ignored() {
    let schema = FilterSchema::new().field("status", FieldType::Text);
    let mut params = HashMap::new();
    params.insert("status".into(), vec!["active".into()]);
    params.insert("password".into(), vec!["secret".into()]);

    let filter = Filter::from_query_params(&params);
    let validated = filter.validate(&schema).unwrap();
    assert_eq!(validated.clauses.len(), 1); // password ignored
}

#[test]
fn filter_sort() {
    let schema = FilterSchema::new()
        .field("status", FieldType::Text)
        .sort_fields(&["created_at", "name"]);

    let mut params = HashMap::new();
    params.insert("sort".into(), vec!["-created_at".into()]);

    let filter = Filter::from_query_params(&params);
    let validated = filter.validate(&schema).unwrap();
    assert_eq!(validated.sort_clause, Some("created_at DESC".into()));
}

#[test]
fn filter_sort_unknown_field_ignored() {
    let schema = FilterSchema::new().sort_fields(&["name"]);
    let mut params = HashMap::new();
    params.insert("sort".into(), vec!["password".into()]);

    let filter = Filter::from_query_params(&params);
    let validated = filter.validate(&schema).unwrap();
    assert_eq!(validated.sort_clause, None);
}

#[test]
fn filter_int_type_validation() {
    let schema = FilterSchema::new().field("age", FieldType::Int);
    let mut params = HashMap::new();
    params.insert("age".into(), vec!["not_a_number".into()]);

    let filter = Filter::from_query_params(&params);
    let result = filter.validate(&schema);
    assert!(result.is_err());
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

// -- SelectBuilder tests --

#[derive(serde::Serialize)]
struct SimpleUser {
    id: String,
    name: String,
    status: String,
}

impl FromRow for SimpleUser {
    fn from_row(row: &libsql::Row) -> modo::error::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            name: row.get(1)?,
            status: row.get(2)?,
        })
    }
}

async fn test_db_with_users() -> ldb::Database {
    let db = test_db().await;
    let conn = db.conn();
    conn.execute(
        "CREATE TABLE items (id TEXT PRIMARY KEY, name TEXT NOT NULL, status TEXT NOT NULL)",
        (),
    )
    .await
    .unwrap();
    for i in 0..50 {
        let status = if i % 2 == 0 { "active" } else { "inactive" };
        conn.execute(
            "INSERT INTO items (id, name, status) VALUES (?1, ?2, ?3)",
            libsql::params![format!("id_{i:04}"), format!("Item {i}"), status],
        )
        .await
        .unwrap();
    }
    db
}

#[tokio::test]
async fn select_fetch_all_with_filter() {
    let db = test_db_with_users().await;
    let conn = db.conn();

    let schema = FilterSchema::new().field("status", FieldType::Text);
    let mut params = HashMap::new();
    params.insert("status".into(), vec!["active".into()]);
    let filter = Filter::from_query_params(&params)
        .validate(&schema)
        .unwrap();

    let items: Vec<SimpleUser> = conn
        .select("SELECT id, name, status FROM items")
        .filter(filter)
        .fetch_all()
        .await
        .unwrap();

    assert_eq!(items.len(), 25); // half are active
    assert!(items.iter().all(|u| u.status == "active"));
}

#[tokio::test]
async fn select_page_with_filter() {
    let db = test_db_with_users().await;
    let conn = db.conn();

    let schema = FilterSchema::new().field("status", FieldType::Text);
    let mut params = HashMap::new();
    params.insert("status".into(), vec!["active".into()]);
    let filter = Filter::from_query_params(&params)
        .validate(&schema)
        .unwrap();

    let page_req = PageRequest {
        page: 1,
        per_page: 10,
    };
    let page: modo::ldb::Page<SimpleUser> = conn
        .select("SELECT id, name, status FROM items")
        .filter(filter)
        .page(page_req)
        .await
        .unwrap();

    assert_eq!(page.items.len(), 10);
    assert_eq!(page.total, 25);
    assert_eq!(page.total_pages, 3);
    assert!(page.has_next);
    assert!(!page.has_prev);
}

#[tokio::test]
async fn select_with_sort() {
    let db = test_db_with_users().await;
    let conn = db.conn();

    let schema = FilterSchema::new()
        .field("status", FieldType::Text)
        .sort_fields(&["name"]);
    let mut params = HashMap::new();
    params.insert("sort".into(), vec!["-name".into()]);
    let filter = Filter::from_query_params(&params)
        .validate(&schema)
        .unwrap();

    let items: Vec<SimpleUser> = conn
        .select("SELECT id, name, status FROM items")
        .filter(filter)
        .fetch_all()
        .await
        .unwrap();

    // Should be sorted by name DESC
    assert!(items[0].name > items[1].name);
}

#[tokio::test]
async fn select_no_filter() {
    let db = test_db_with_users().await;
    let conn = db.conn();

    let page_req = PageRequest {
        page: 2,
        per_page: 20,
    };
    let page: modo::ldb::Page<SimpleUser> = conn
        .select("SELECT id, name, status FROM items")
        .page(page_req)
        .await
        .unwrap();

    assert_eq!(page.items.len(), 20);
    assert_eq!(page.total, 50);
    assert_eq!(page.page, 2);
    assert!(page.has_prev);
    assert!(page.has_next);
}

// -- Cursor pagination tests --

#[tokio::test]
async fn select_cursor_first_page() {
    let db = test_db_with_users().await;
    let conn = db.conn();

    let req = ldb::CursorRequest {
        after: None,
        per_page: 10,
    };
    let page: ldb::CursorPage<SimpleUser> = conn
        .select("SELECT id, name, status FROM items")
        .cursor(req)
        .await
        .unwrap();

    assert_eq!(page.items.len(), 10);
    assert!(page.has_more);
    assert!(page.next_cursor.is_some());
}

#[tokio::test]
async fn select_cursor_with_after() {
    let db = test_db_with_users().await;
    let conn = db.conn();

    // Get first page
    let req = ldb::CursorRequest {
        after: None,
        per_page: 10,
    };
    let first: ldb::CursorPage<SimpleUser> = conn
        .select("SELECT id, name, status FROM items")
        .cursor(req)
        .await
        .unwrap();

    // Get second page using cursor
    let req = ldb::CursorRequest {
        after: first.next_cursor.clone(),
        per_page: 10,
    };
    let second: ldb::CursorPage<SimpleUser> = conn
        .select("SELECT id, name, status FROM items")
        .cursor(req)
        .await
        .unwrap();

    assert_eq!(second.items.len(), 10);
    assert!(second.has_more);
    // No overlap between pages
    let first_ids: Vec<_> = first.items.iter().map(|u| &u.id).collect();
    assert!(!second.items.iter().any(|u| first_ids.contains(&&u.id)));
}

#[tokio::test]
async fn select_cursor_last_page() {
    let db = test_db_with_users().await;
    let conn = db.conn();

    // Request more than available (50 items, request 100)
    let req = ldb::CursorRequest {
        after: None,
        per_page: 100,
    };
    let page: ldb::CursorPage<SimpleUser> = conn
        .select("SELECT id, name, status FROM items")
        .cursor(req)
        .await
        .unwrap();

    assert_eq!(page.items.len(), 50);
    assert!(!page.has_more);
    assert!(page.next_cursor.is_none());
}

#[tokio::test]
async fn select_cursor_with_filter() {
    let db = test_db_with_users().await;
    let conn = db.conn();

    let schema = FilterSchema::new().field("status", FieldType::Text);
    let mut params = HashMap::new();
    params.insert("status".into(), vec!["active".into()]);
    let filter = Filter::from_query_params(&params)
        .validate(&schema)
        .unwrap();

    let req = ldb::CursorRequest {
        after: None,
        per_page: 10,
    };
    let page: ldb::CursorPage<SimpleUser> = conn
        .select("SELECT id, name, status FROM items")
        .filter(filter)
        .cursor(req)
        .await
        .unwrap();

    assert_eq!(page.items.len(), 10);
    assert!(page.items.iter().all(|u| u.status == "active"));
    assert!(page.has_more); // 25 active items total, got 10
}

#[tokio::test]
async fn select_cursor_custom_column() {
    let db = test_db().await;
    let conn = db.conn();

    conn.execute(
        "CREATE TABLE docs (ulid TEXT PRIMARY KEY, title TEXT NOT NULL)",
        (),
    )
    .await
    .unwrap();
    for i in 0..30 {
        conn.execute(
            "INSERT INTO docs (ulid, title) VALUES (?1, ?2)",
            libsql::params![format!("ulid_{i:04}"), format!("Doc {i}")],
        )
        .await
        .unwrap();
    }

    let req = ldb::CursorRequest {
        after: None,
        per_page: 10,
    };
    let page: ldb::CursorPage<DocRow> = conn
        .select("SELECT ulid, title FROM docs")
        .cursor_column("ulid")
        .cursor(req)
        .await
        .unwrap();

    assert_eq!(page.items.len(), 10);
    assert!(page.has_more);
    assert!(page.next_cursor.is_some());
    // Verify ordering: first page should be ulid_0000..ulid_0009
    assert_eq!(page.items[0].ulid, "ulid_0000");
    assert_eq!(page.items[9].ulid, "ulid_0009");
    // All items in ascending order
    for w in page.items.windows(2) {
        assert!(w[0].ulid < w[1].ulid);
    }

    // Second page using cursor
    let req = ldb::CursorRequest {
        after: page.next_cursor.clone(),
        per_page: 10,
    };
    let page2: ldb::CursorPage<DocRow> = conn
        .select("SELECT ulid, title FROM docs")
        .cursor_column("ulid")
        .cursor(req)
        .await
        .unwrap();

    assert_eq!(page2.items.len(), 10);
    assert!(page2.has_more);
    // No overlap
    let first_ids: Vec<_> = page.items.iter().map(|d| &d.ulid).collect();
    assert!(!page2.items.iter().any(|d| first_ids.contains(&&d.ulid)));
}

#[derive(serde::Serialize)]
struct DocRow {
    ulid: String,
    title: String,
}

impl FromRow for DocRow {
    fn from_row(row: &libsql::Row) -> Result<Self> {
        Ok(Self {
            ulid: row.get(0)?,
            title: row.get(1)?,
        })
    }
}

#[tokio::test]
async fn select_cursor_missing_column_errors() {
    let db = test_db_with_users().await;
    let conn = db.conn();

    let req = ldb::CursorRequest {
        after: None,
        per_page: 10,
    };
    let result: Result<ldb::CursorPage<SimpleUser>> = conn
        .select("SELECT id, name, status FROM items")
        .cursor_column("nonexistent")
        .cursor(req)
        .await;

    assert!(result.is_err());
}
