use std::collections::HashMap;

use http::StatusCode;
use modo::db;
use modo::db::{
    ColumnMap, ConnExt, ConnQueryExt, FieldType, Filter, FilterSchema, FromRow, PageRequest,
};
use modo::error::Result;

#[tokio::test]
async fn connect_in_memory() {
    let config = db::Config {
        path: ":memory:".to_string(),
        ..Default::default()
    };
    let db = db::connect(&config).await.unwrap();
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

    let config = db::Config {
        path: db_path.to_string_lossy().to_string(),
        ..Default::default()
    };
    let db = db::connect(&config).await.unwrap();

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
    let config = db::Config {
        path: ":memory:".to_string(),
        ..Default::default()
    };
    let db = db::connect(&config).await.unwrap();
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

    db::migrate(conn, "tests/fixtures/db_migrations")
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

    db::migrate(conn, "tests/fixtures/db_migrations")
        .await
        .unwrap();
    // Run again — should not error
    db::migrate(conn, "tests/fixtures/db_migrations")
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
    db::migrate(conn, "tests/fixtures/nonexistent")
        .await
        .unwrap();
}

// -- Managed shutdown --

#[tokio::test]
async fn managed_shutdown() {
    let db = test_db().await;
    let managed = db::managed(db);
    // ManagedDatabase implements Task — shutdown should succeed
    use modo::runtime::Task;
    managed.shutdown().await.unwrap();
}

// -- Pagination --

#[tokio::test]
async fn page_request_clamp() {
    let config = db::PaginationConfig {
        default_per_page: 20,
        max_per_page: 50,
    };

    let mut req = db::PageRequest {
        page: 0,
        per_page: 0,
    };
    req.clamp(&config);
    assert_eq!(req.page, 1);
    assert_eq!(req.per_page, 20);

    let mut req = db::PageRequest {
        page: 3,
        per_page: 200,
    };
    req.clamp(&config);
    assert_eq!(req.page, 3);
    assert_eq!(req.per_page, 50);
}

#[tokio::test]
async fn page_new_calculates_fields() {
    let page: db::Page<String> = db::Page::new(vec!["a".into(), "b".into()], 5, 2, 2);
    assert_eq!(page.total_pages, 3);
    assert!(page.has_next);
    assert!(page.has_prev);
}

#[tokio::test]
async fn cursor_request_clamp() {
    let config = db::PaginationConfig::default();

    let mut req = db::CursorRequest {
        after: None,
        per_page: 0,
    };
    req.clamp(&config);
    assert_eq!(req.per_page, 20);

    let mut req = db::CursorRequest {
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
    assert_eq!(validated.clauses[0], "\"status\" = ?");
    assert_eq!(validated.params.len(), 1);
}

#[test]
fn filter_in_multiple_values() {
    let schema = FilterSchema::new().field("status", FieldType::Text);
    let mut params = HashMap::new();
    params.insert("status".into(), vec!["active".into(), "pending".into()]);

    let filter = Filter::from_query_params(&params);
    let validated = filter.validate(&schema).unwrap();
    assert_eq!(validated.clauses[0], "\"status\" IN (?, ?)");
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
    assert_eq!(validated.clauses[0], "\"deleted_at\" IS NULL");
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
    assert_eq!(validated.sort_clause, Some("\"created_at\" DESC".into()));
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
fn filter_sort_multi_column() {
    let schema = FilterSchema::new()
        .field("status", FieldType::Text)
        .sort_fields(&["priority", "end_date", "name"]);

    let mut params = HashMap::new();
    params.insert(
        "sort".into(),
        vec!["priority".into(), "-end_date".into(), "name".into()],
    );

    let filter = Filter::from_query_params(&params);
    let validated = filter.validate(&schema).unwrap();
    assert_eq!(
        validated.sort_clause,
        Some("\"priority\" ASC, \"end_date\" DESC, \"name\" ASC".into())
    );
}

#[test]
fn filter_sort_duplicate_first_wins() {
    let schema = FilterSchema::new().sort_fields(&["name"]);

    let mut params = HashMap::new();
    params.insert("sort".into(), vec!["name".into(), "-name".into()]);

    let filter = Filter::from_query_params(&params);
    let validated = filter.validate(&schema).unwrap();
    assert_eq!(validated.sort_clause, Some("\"name\" ASC".into()));
}

#[test]
fn filter_sort_unknown_fields_dropped() {
    let schema = FilterSchema::new().sort_fields(&["name", "created_at"]);

    let mut params = HashMap::new();
    params.insert(
        "sort".into(),
        vec![
            "unknown".into(),
            "-name".into(),
            "password".into(),
            "created_at".into(),
        ],
    );

    let filter = Filter::from_query_params(&params);
    let validated = filter.validate(&schema).unwrap();
    assert_eq!(
        validated.sort_clause,
        Some("\"name\" DESC, \"created_at\" ASC".into())
    );
}

#[test]
fn filter_sort_all_unknown_produces_none() {
    let schema = FilterSchema::new().sort_fields(&["name"]);

    let mut params = HashMap::new();
    params.insert("sort".into(), vec!["unknown".into(), "password".into()]);

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

async fn test_db() -> db::Database {
    let config = db::Config {
        path: ":memory:".to_string(),
        ..Default::default()
    };
    db::connect(&config).await.unwrap()
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

async fn test_db_with_users() -> db::Database {
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
    let page: modo::db::Page<SimpleUser> = conn
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
async fn select_with_multi_column_sort() {
    let db = test_db().await;
    let conn = db.conn();

    conn.execute(
        "CREATE TABLE tasks (id TEXT PRIMARY KEY, name TEXT NOT NULL, priority INTEGER NOT NULL, status TEXT NOT NULL)",
        (),
    )
    .await
    .unwrap();

    // Insert rows with varying priority and name to verify multi-column ordering
    for (id, name, priority, status) in [
        ("t1", "Deploy", 2, "active"),
        ("t2", "Review", 1, "active"),
        ("t3", "Build", 2, "active"),
        ("t4", "Audit", 1, "active"),
    ] {
        conn.execute(
            "INSERT INTO tasks (id, name, priority, status) VALUES (?1, ?2, ?3, ?4)",
            libsql::params![id, name, priority, status],
        )
        .await
        .unwrap();
    }

    let schema = FilterSchema::new()
        .field("status", FieldType::Text)
        .sort_fields(&["priority", "name"]);

    // Sort by priority ASC, then name ASC as tiebreaker
    let mut params = HashMap::new();
    params.insert("sort".into(), vec!["priority".into(), "name".into()]);
    params.insert("status".into(), vec!["active".into()]);
    let filter = Filter::from_query_params(&params)
        .validate(&schema)
        .unwrap();

    struct Task {
        name: String,
        priority: i64,
    }
    impl FromRow for Task {
        fn from_row(row: &libsql::Row) -> Result<Self> {
            Ok(Self {
                name: row.get(0)?,
                priority: row.get(1)?,
            })
        }
    }

    let items: Vec<Task> = conn
        .select("SELECT name, priority FROM tasks")
        .filter(filter)
        .fetch_all()
        .await
        .unwrap();

    assert_eq!(items.len(), 4);
    // priority ASC: 1, 1, 2, 2 — then name ASC within same priority
    assert_eq!(items[0].priority, 1);
    assert_eq!(items[0].name, "Audit");
    assert_eq!(items[1].priority, 1);
    assert_eq!(items[1].name, "Review");
    assert_eq!(items[2].priority, 2);
    assert_eq!(items[2].name, "Build");
    assert_eq!(items[3].priority, 2);
    assert_eq!(items[3].name, "Deploy");
}

#[tokio::test]
async fn select_no_filter() {
    let db = test_db_with_users().await;
    let conn = db.conn();

    let page_req = PageRequest {
        page: 2,
        per_page: 20,
    };
    let page: modo::db::Page<SimpleUser> = conn
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

    let req = db::CursorRequest {
        after: None,
        per_page: 10,
    };
    let page: db::CursorPage<SimpleUser> = conn
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
    let req = db::CursorRequest {
        after: None,
        per_page: 10,
    };
    let first: db::CursorPage<SimpleUser> = conn
        .select("SELECT id, name, status FROM items")
        .cursor(req)
        .await
        .unwrap();

    // Get second page using cursor
    let req = db::CursorRequest {
        after: first.next_cursor.clone(),
        per_page: 10,
    };
    let second: db::CursorPage<SimpleUser> = conn
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
    let req = db::CursorRequest {
        after: None,
        per_page: 100,
    };
    let page: db::CursorPage<SimpleUser> = conn
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

    let req = db::CursorRequest {
        after: None,
        per_page: 10,
    };
    let page: db::CursorPage<SimpleUser> = conn
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

    let req = db::CursorRequest {
        after: None,
        per_page: 10,
    };
    let page: db::CursorPage<DocRow> = conn
        .select("SELECT ulid, title FROM docs")
        .cursor_column("ulid")
        .oldest_first()
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
    let req = db::CursorRequest {
        after: page.next_cursor.clone(),
        per_page: 10,
    };
    let page2: db::CursorPage<DocRow> = conn
        .select("SELECT ulid, title FROM docs")
        .cursor_column("ulid")
        .oldest_first()
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

    let req = db::CursorRequest {
        after: None,
        per_page: 10,
    };
    let result: Result<db::CursorPage<SimpleUser>> = conn
        .select("SELECT id, name, status FROM items")
        .cursor_column("nonexistent")
        .cursor(req)
        .await;

    assert!(result.is_err());
}

#[tokio::test]
async fn cursor_newest_first() {
    let db = test_db_with_users().await;
    let conn = db.conn();

    // Default is newest-first (DESC) — should return highest IDs first
    let page: db::CursorPage<SimpleUser> = conn
        .select("SELECT id, name, status FROM items")
        .cursor(db::CursorRequest {
            after: None,
            per_page: 5,
        })
        .await
        .unwrap();

    assert_eq!(page.items.len(), 5);
    assert!(page.has_more);
    // First item should have the highest ID (newest) — id_0049 > id_0000
    assert!(
        page.items[0].id > page.items[4].id,
        "newest-first: first ID should be greater than last, got {} and {}",
        page.items[0].id,
        page.items[4].id
    );
}

#[tokio::test]
async fn cursor_oldest_first() {
    let db = test_db_with_users().await;
    let conn = db.conn();

    let page: db::CursorPage<SimpleUser> = conn
        .select("SELECT id, name, status FROM items")
        .oldest_first()
        .cursor(db::CursorRequest {
            after: None,
            per_page: 5,
        })
        .await
        .unwrap();

    assert_eq!(page.items.len(), 5);
    assert!(page.has_more);
    // First item should have the lowest ID (oldest) — id_0000 < id_0049
    assert!(
        page.items[0].id < page.items[4].id,
        "oldest-first: first ID should be less than last, got {} and {}",
        page.items[0].id,
        page.items[4].id
    );
}

// -- SelectBuilder fetch_one / fetch_optional tests --

#[tokio::test]
async fn select_fetch_one_found() {
    let db = test_db_with_users().await;
    let conn = db.conn();

    let user: SimpleUser = conn
        .select("SELECT id, name, status FROM items")
        .fetch_one()
        .await
        .unwrap();
    assert!(!user.id.is_empty());
}

#[tokio::test]
async fn select_fetch_one_not_found() {
    let db = test_db_with_users().await;
    let conn = db.conn();

    let schema = FilterSchema::new().field("status", FieldType::Text);
    let mut params = HashMap::new();
    params.insert("status".into(), vec!["nonexistent".into()]);
    let filter = Filter::from_query_params(&params)
        .validate(&schema)
        .unwrap();

    let result: Result<SimpleUser> = conn
        .select("SELECT id, name, status FROM items")
        .filter(filter)
        .fetch_one()
        .await;

    let err = result.err().unwrap();
    assert_eq!(err.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn select_fetch_optional_some() {
    let db = test_db_with_users().await;
    let conn = db.conn();

    let user: Option<SimpleUser> = conn
        .select("SELECT id, name, status FROM items")
        .fetch_optional()
        .await
        .unwrap();
    assert!(user.is_some());
}

#[tokio::test]
async fn select_fetch_optional_none() {
    let db = test_db_with_users().await;
    let conn = db.conn();

    let schema = FilterSchema::new().field("status", FieldType::Text);
    let mut params = HashMap::new();
    params.insert("status".into(), vec!["nonexistent".into()]);
    let filter = Filter::from_query_params(&params)
        .validate(&schema)
        .unwrap();

    let user: Option<SimpleUser> = conn
        .select("SELECT id, name, status FROM items")
        .filter(filter)
        .fetch_optional()
        .await
        .unwrap();
    assert!(user.is_none());
}

// -- Migration checksum mismatch test --

#[tokio::test]
async fn migrate_checksum_mismatch() {
    let dir = tempfile::tempdir().unwrap();
    let migration_path = dir.path().join("001_init.sql");
    std::fs::write(&migration_path, "CREATE TABLE t (id TEXT PRIMARY KEY);").unwrap();

    let db = test_db().await;
    let conn = db.conn();
    db::migrate(conn, dir.path().to_str().unwrap())
        .await
        .unwrap();

    // Overwrite the migration file with different content
    std::fs::write(
        &migration_path,
        "CREATE TABLE t (id TEXT PRIMARY KEY, name TEXT);",
    )
    .unwrap();

    let result = db::migrate(conn, dir.path().to_str().unwrap()).await;
    let err = result.unwrap_err();
    assert!(
        format!("{err}").contains("checksum mismatch"),
        "expected checksum mismatch error, got: {err}"
    );
}

// -- Error mapping tests --

#[tokio::test]
async fn error_unique_constraint_maps_to_conflict() {
    let db = test_db().await;
    let conn = db.conn();
    seed_users(conn).await;

    // Insert duplicate primary key
    let result = conn
        .execute(
            "INSERT INTO users (id, name, email) VALUES ('u1', 'Dup', 'dup@test.com')",
            (),
        )
        .await;
    let err = modo::error::Error::from(result.unwrap_err());
    assert_eq!(err.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn error_foreign_key_maps_to_bad_request() {
    let db = test_db().await;
    let conn = db.conn();
    conn.execute("CREATE TABLE parents (id TEXT PRIMARY KEY)", ())
        .await
        .unwrap();
    conn.execute(
        "CREATE TABLE children (id TEXT PRIMARY KEY, parent_id TEXT NOT NULL REFERENCES parents(id))",
        (),
    )
    .await
    .unwrap();

    let result = conn
        .execute(
            "INSERT INTO children (id, parent_id) VALUES ('c1', 'nonexistent')",
            (),
        )
        .await;
    let err = modo::error::Error::from(result.unwrap_err());
    assert_eq!(err.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn error_not_found_on_query_one() {
    let db = test_db().await;
    let conn = db.conn();
    seed_users(conn).await;

    let result: Result<User> = conn
        .query_one(
            "SELECT id, name, email FROM users WHERE id = ?1",
            libsql::params!["nonexistent"],
        )
        .await;
    let err = result.err().unwrap();
    assert_eq!(err.status(), StatusCode::NOT_FOUND);
}

// -- FromValue edge case tests --

#[tokio::test]
async fn from_value_type_mismatch() {
    let db = test_db().await;
    let conn = db.conn();
    conn.execute("CREATE TABLE vals (v TEXT)", ())
        .await
        .unwrap();
    conn.execute("INSERT INTO vals (v) VALUES ('hello')", ())
        .await
        .unwrap();

    let mut rows = conn.query("SELECT v FROM vals", ()).await.unwrap();
    let row = rows.next().await.unwrap().unwrap();
    let cols = ColumnMap::from_row(&row);
    let result = cols.get::<i64>(&row, "v");
    assert!(result.is_err());
}

#[tokio::test]
async fn from_value_null_non_optional() {
    let db = test_db().await;
    let conn = db.conn();
    conn.execute("CREATE TABLE vals (v TEXT)", ())
        .await
        .unwrap();
    conn.execute("INSERT INTO vals (v) VALUES (NULL)", ())
        .await
        .unwrap();

    let mut rows = conn.query("SELECT v FROM vals", ()).await.unwrap();
    let row = rows.next().await.unwrap().unwrap();
    let cols = ColumnMap::from_row(&row);
    let result = cols.get::<String>(&row, "v");
    assert!(result.is_err());
}

#[tokio::test]
async fn from_value_option_some() {
    let db = test_db().await;
    let conn = db.conn();
    conn.execute("CREATE TABLE vals (v TEXT)", ())
        .await
        .unwrap();
    conn.execute("INSERT INTO vals (v) VALUES ('hello')", ())
        .await
        .unwrap();

    let mut rows = conn.query("SELECT v FROM vals", ()).await.unwrap();
    let row = rows.next().await.unwrap().unwrap();
    let cols = ColumnMap::from_row(&row);
    let result: Option<String> = cols.get(&row, "v").unwrap();
    assert_eq!(result, Some("hello".to_string()));
}

#[tokio::test]
async fn from_value_option_none() {
    let db = test_db().await;
    let conn = db.conn();
    conn.execute("CREATE TABLE vals (v TEXT)", ())
        .await
        .unwrap();
    conn.execute("INSERT INTO vals (v) VALUES (NULL)", ())
        .await
        .unwrap();

    let mut rows = conn.query("SELECT v FROM vals", ()).await.unwrap();
    let row = rows.next().await.unwrap().unwrap();
    let cols = ColumnMap::from_row(&row);
    let result: Option<String> = cols.get(&row, "v").unwrap();
    assert_eq!(result, None);
}

#[tokio::test]
async fn from_value_blob_roundtrip() {
    let db = test_db().await;
    let conn = db.conn();
    conn.execute("CREATE TABLE vals (v BLOB)", ())
        .await
        .unwrap();
    conn.execute(
        "INSERT INTO vals (v) VALUES (?1)",
        libsql::params![vec![0xDE_u8, 0xAD, 0xBE, 0xEF]],
    )
    .await
    .unwrap();

    let mut rows = conn.query("SELECT v FROM vals", ()).await.unwrap();
    let row = rows.next().await.unwrap().unwrap();
    let cols = ColumnMap::from_row(&row);
    let result: Vec<u8> = cols.get(&row, "v").unwrap();
    assert_eq!(result, vec![0xDE, 0xAD, 0xBE, 0xEF]);
}

#[tokio::test]
async fn from_value_bool_variants() {
    let db = test_db().await;
    let conn = db.conn();
    conn.execute("CREATE TABLE vals (v INTEGER)", ())
        .await
        .unwrap();
    conn.execute("INSERT INTO vals (v) VALUES (0)", ())
        .await
        .unwrap();
    conn.execute("INSERT INTO vals (v) VALUES (1)", ())
        .await
        .unwrap();
    conn.execute("INSERT INTO vals (v) VALUES (42)", ())
        .await
        .unwrap();

    let mut rows = conn
        .query("SELECT v FROM vals ORDER BY v", ())
        .await
        .unwrap();
    let row0 = rows.next().await.unwrap().unwrap();
    let cols0 = ColumnMap::from_row(&row0);
    assert!(!cols0.get::<bool>(&row0, "v").unwrap());

    let row1 = rows.next().await.unwrap().unwrap();
    let cols1 = ColumnMap::from_row(&row1);
    assert!(cols1.get::<bool>(&row1, "v").unwrap());

    let row42 = rows.next().await.unwrap().unwrap();
    let cols42 = ColumnMap::from_row(&row42);
    assert!(cols42.get::<bool>(&row42, "v").unwrap());
}

// -- Filter edge case tests --

#[test]
fn filter_float_type_validation() {
    let schema = FilterSchema::new().field("score", FieldType::Float);
    let mut params = HashMap::new();
    params.insert("score".into(), vec!["not_a_float".into()]);

    let filter = Filter::from_query_params(&params);
    let result = filter.validate(&schema);
    assert!(result.is_err());
}

#[test]
fn filter_bool_conversion() {
    let schema = FilterSchema::new().field("active", FieldType::Bool);

    // "true", "1", "yes" all produce Integer(1)
    for val in &["true", "1", "yes"] {
        let mut params = HashMap::new();
        params.insert("active".into(), vec![val.to_string()]);
        let filter = Filter::from_query_params(&params);
        let validated = filter.validate(&schema).unwrap();
        assert_eq!(
            validated.params[0],
            libsql::Value::from(1_i32),
            "expected Integer(1) for input '{val}'"
        );
    }

    // "false", "0", "no" all produce Integer(0)
    for val in &["false", "0", "no"] {
        let mut params = HashMap::new();
        params.insert("active".into(), vec![val.to_string()]);
        let filter = Filter::from_query_params(&params);
        let validated = filter.validate(&schema).unwrap();
        assert_eq!(
            validated.params[0],
            libsql::Value::from(0_i32),
            "expected Integer(0) for input '{val}'"
        );
    }
}

#[test]
fn filter_explicit_eq_operator_is_unknown() {
    // "status.eq=active" — "eq" is not a recognized operator, so it's skipped
    let schema = FilterSchema::new().field("status", FieldType::Text);
    let mut params = HashMap::new();
    params.insert("status.eq".into(), vec!["active".into()]);

    let filter = Filter::from_query_params(&params);
    let validated = filter.validate(&schema).unwrap();
    assert!(validated.clauses.is_empty());
}

#[test]
fn filter_empty_value() {
    let schema = FilterSchema::new().field("status", FieldType::Text);
    let mut params = HashMap::new();
    params.insert("status".into(), vec![String::new()]);

    let filter = Filter::from_query_params(&params);
    let validated = filter.validate(&schema).unwrap();
    assert_eq!(validated.params[0], libsql::Value::from(String::new()));
}

// -- execute_raw test --

#[tokio::test]
async fn conn_ext_execute_returns_affected_rows() {
    let db = test_db().await;
    let conn = db.conn();
    seed_users(conn).await;

    // INSERT returns 1
    let inserted = conn
        .execute_raw(
            "INSERT INTO users (id, name, email) VALUES ('u3', 'Carol', 'carol@test.com')",
            (),
        )
        .await
        .unwrap();
    assert_eq!(inserted, 1);

    // UPDATE returns affected count
    let updated = conn
        .execute_raw("UPDATE users SET name = 'Updated'", ())
        .await
        .unwrap();
    assert_eq!(updated, 3);

    // DELETE returns 1
    let deleted = conn
        .execute_raw("DELETE FROM users WHERE id = 'u3'", ())
        .await
        .unwrap();
    assert_eq!(deleted, 1);
}
