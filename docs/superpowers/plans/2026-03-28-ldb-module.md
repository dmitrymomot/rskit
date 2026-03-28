# ldb Module Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** New libsql-based database module for modo with single-connection architecture, query helpers, safe filter DSL, pagination, and migration runner.

**Architecture:** Feature-gated `ldb` module (`src/ldb/`) with 12 files. ConnExt extension trait on `libsql::Connection` and `libsql::Transaction`. SelectBuilder composes Filter + pagination. Database holds a single Arc'd connection. All helpers are opt-in — raw libsql always accessible.

**Tech Stack:** libsql 0.9 (local mode), serde (config deserialization), axum (extractors)

---

## File Map

| File | Action | Responsibility |
|------|--------|---------------|
| `Cargo.toml` | Modify | Add libsql dep + `ldb` feature |
| `src/lib.rs` | Modify | Add `ldb` module declaration + re-exports |
| `src/ldb/mod.rs` | Create | Module exports |
| `src/ldb/error.rs` | Create | libsql::Error → modo::Error conversion |
| `src/ldb/config.rs` | Create | Config struct, PRAGMA enums, defaults |
| `src/ldb/database.rs` | Create | Database struct (Arc<Inner>) |
| `src/ldb/connect.rs` | Create | connect() function, PRAGMA application |
| `src/ldb/from_row.rs` | Create | FromRow trait, ColumnMap |
| `src/ldb/conn.rs` | Create | ConnExt extension trait |
| `src/ldb/migrate.rs` | Create | Migration runner |
| `src/ldb/managed.rs` | Create | Task impl for graceful shutdown |
| `src/ldb/page.rs` | Create | Page, CursorPage, PageRequest, CursorRequest, PaginationConfig |
| `src/ldb/filter.rs` | Create | FilterSchema, Filter, ValidatedFilter, extraction |
| `src/ldb/select.rs` | Create | SelectBuilder |
| `tests/ldb_test.rs` | Create | Integration tests |

---

### Task 1: Foundation — Cargo.toml, mod.rs, error.rs

**Files:**
- Modify: `Cargo.toml`
- Create: `src/ldb/mod.rs`
- Create: `src/ldb/error.rs`

- [ ] **Step 1: Add libsql dependency and ldb feature to Cargo.toml**

In the `[dependencies]` section, add after the sqlx block:

```toml
libsql = { version = "0.9", optional = true, default-features = false }
```

In the `[features]` section, add:

```toml
ldb = ["dep:libsql"]
```

Add `"ldb"` to the `full` feature list.

- [ ] **Step 2: Create src/ldb/error.rs**

```rust
use crate::error::Error;

impl From<libsql::Error> for Error {
    fn from(err: libsql::Error) -> Self {
        match &err {
            libsql::Error::SqliteFailure(code, msg) => {
                // SQLite extended error codes
                // SQLITE_CONSTRAINT_UNIQUE = 2067
                // SQLITE_CONSTRAINT_FOREIGNKEY = 787
                // SQLITE_CONSTRAINT_PRIMARYKEY = 1555
                match *code {
                    2067 | 1555 => Error::conflict("record already exists").chain(err),
                    787 => Error::bad_request("foreign key violation").chain(err),
                    _ => Error::internal(format!("database error: {msg}")).chain(err),
                }
            }
            libsql::Error::QueryReturnedNoRows => Error::not_found("record not found"),
            libsql::Error::NullValue => Error::bad_request("unexpected null value"),
            libsql::Error::ConnectionFailed(msg) => {
                Error::internal(format!("database connection failed: {msg}"))
            }
            libsql::Error::InvalidColumnIndex => {
                Error::internal("invalid column index")
            }
            libsql::Error::InvalidColumnType => {
                Error::internal("invalid column type")
            }
            _ => Error::internal("database error").chain(err),
        }
    }
}
```

- [ ] **Step 3: Create src/ldb/mod.rs (skeleton)**

```rust
mod error;
```

- [ ] **Step 4: Add module declaration to src/lib.rs**

Add with the other module declarations:

```rust
#[cfg(feature = "ldb")]
pub mod ldb;
```

- [ ] **Step 5: Verify it compiles**

```bash
cargo check --features ldb
```

Expected: Compiles with no errors.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml src/lib.rs src/ldb/
git commit -m "feat(ldb): add module skeleton and error conversion"
```

---

### Task 2: Config

**Files:**
- Create: `src/ldb/config.rs`
- Modify: `src/ldb/mod.rs`

- [ ] **Step 1: Create src/ldb/config.rs**

```rust
use serde::Deserialize;

/// Database configuration. All fields have sensible defaults.
#[derive(Debug, Deserialize)]
pub struct Config {
    /// Database file path.
    #[serde(default = "defaults::path")]
    pub path: String,

    /// Migration directory. If set, migrations run on connect.
    #[serde(default)]
    pub migrations: Option<String>,

    /// Busy timeout in milliseconds.
    #[serde(default = "defaults::busy_timeout")]
    pub busy_timeout: u64,

    /// Cache size in KB (applied as PRAGMA cache_size = -N).
    #[serde(default = "defaults::cache_size")]
    pub cache_size: i64,

    /// Memory-mapped I/O size in bytes.
    #[serde(default = "defaults::mmap_size")]
    pub mmap_size: u64,

    /// WAL journal mode.
    #[serde(default = "defaults::journal_mode")]
    pub journal_mode: JournalMode,

    /// Synchronous mode.
    #[serde(default = "defaults::synchronous")]
    pub synchronous: SynchronousMode,

    /// Foreign key enforcement.
    #[serde(default = "defaults::foreign_keys")]
    pub foreign_keys: bool,

    /// Temp store location.
    #[serde(default = "defaults::temp_store")]
    pub temp_store: TempStore,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            path: defaults::path(),
            migrations: None,
            busy_timeout: defaults::busy_timeout(),
            cache_size: defaults::cache_size(),
            mmap_size: defaults::mmap_size(),
            journal_mode: defaults::journal_mode(),
            synchronous: defaults::synchronous(),
            foreign_keys: defaults::foreign_keys(),
            temp_store: defaults::temp_store(),
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum JournalMode {
    #[default]
    Wal,
    Delete,
    Truncate,
    Memory,
    Off,
}

impl JournalMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Wal => "WAL",
            Self::Delete => "DELETE",
            Self::Truncate => "TRUNCATE",
            Self::Memory => "MEMORY",
            Self::Off => "OFF",
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SynchronousMode {
    Off,
    #[default]
    Normal,
    Full,
    Extra,
}

impl SynchronousMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "OFF",
            Self::Normal => "NORMAL",
            Self::Full => "FULL",
            Self::Extra => "EXTRA",
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum TempStore {
    Default,
    File,
    #[default]
    Memory,
}

impl TempStore {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Default => "DEFAULT",
            Self::File => "FILE",
            Self::Memory => "MEMORY",
        }
    }
}

mod defaults {
    use super::*;

    pub fn path() -> String {
        "data/app.db".to_string()
    }

    pub fn busy_timeout() -> u64 {
        5000
    }

    pub fn cache_size() -> i64 {
        16384
    }

    pub fn mmap_size() -> u64 {
        268_435_456 // 256 MB
    }

    pub fn journal_mode() -> JournalMode {
        JournalMode::Wal
    }

    pub fn synchronous() -> SynchronousMode {
        SynchronousMode::Normal
    }

    pub fn foreign_keys() -> bool {
        true
    }

    pub fn temp_store() -> TempStore {
        TempStore::Memory
    }
}
```

- [ ] **Step 2: Update src/ldb/mod.rs**

```rust
mod error;

mod config;
pub use config::{Config, JournalMode, SynchronousMode, TempStore};
```

- [ ] **Step 3: Verify it compiles**

```bash
cargo check --features ldb
```

- [ ] **Step 4: Commit**

```bash
git add src/ldb/
git commit -m "feat(ldb): add Config with PRAGMA enums and defaults"
```

---

### Task 3: Database + Connect

**Files:**
- Create: `src/ldb/database.rs`
- Create: `src/ldb/connect.rs`
- Modify: `src/ldb/mod.rs`
- Test: `tests/ldb_test.rs`

- [ ] **Step 1: Create src/ldb/database.rs**

```rust
use std::sync::Arc;

/// Single-connection database handle. Clone-able (Arc internally).
#[derive(Clone)]
pub struct Database {
    inner: Arc<Inner>,
}

struct Inner {
    #[allow(dead_code)]
    db: libsql::Database,
    conn: libsql::Connection,
}

impl Database {
    pub(crate) fn new(db: libsql::Database, conn: libsql::Connection) -> Self {
        Self {
            inner: Arc::new(Inner { db, conn }),
        }
    }

    /// Returns a reference to the underlying libsql connection.
    pub fn conn(&self) -> &libsql::Connection {
        &self.inner.conn
    }
}
```

- [ ] **Step 2: Create src/ldb/connect.rs**

```rust
use crate::error::Result;

use super::config::Config;
use super::database::Database;

/// Open a database, apply PRAGMAs, and optionally run migrations.
pub async fn connect(config: &Config) -> Result<Database> {
    // Create parent directories if needed
    if let Some(parent) = std::path::Path::new(&config.path).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| {
                crate::error::Error::internal(format!(
                    "failed to create database directory: {parent:?}"
                ))
                .chain(e)
            })?;
        }
    }

    let db = libsql::Builder::new_local(&config.path)
        .build()
        .await
        .map_err(crate::error::Error::from)?;

    let conn = db.connect().map_err(crate::error::Error::from)?;

    // Apply PRAGMAs (use query() because PRAGMAs return rows in libsql)
    conn.query(
        &format!("PRAGMA journal_mode={}", config.journal_mode.as_str()),
        (),
    )
    .await
    .map_err(crate::error::Error::from)?;

    conn.query(
        &format!("PRAGMA synchronous={}", config.synchronous.as_str()),
        (),
    )
    .await
    .map_err(crate::error::Error::from)?;

    conn.query(
        &format!("PRAGMA busy_timeout={}", config.busy_timeout),
        (),
    )
    .await
    .map_err(crate::error::Error::from)?;

    conn.query(
        &format!("PRAGMA cache_size=-{}", config.cache_size),
        (),
    )
    .await
    .map_err(crate::error::Error::from)?;

    conn.query(&format!("PRAGMA mmap_size={}", config.mmap_size), ())
        .await
        .map_err(crate::error::Error::from)?;

    conn.query(
        &format!(
            "PRAGMA foreign_keys={}",
            if config.foreign_keys { "ON" } else { "OFF" }
        ),
        (),
    )
    .await
    .map_err(crate::error::Error::from)?;

    conn.query(
        &format!("PRAGMA temp_store={}", config.temp_store.as_str()),
        (),
    )
    .await
    .map_err(crate::error::Error::from)?;

    // Run migrations if configured
    if let Some(ref migrations_dir) = config.migrations {
        super::migrate::migrate(&conn, migrations_dir).await?;
    }

    Ok(Database::new(db, conn))
}
```

- [ ] **Step 3: Create a stub src/ldb/migrate.rs** (full implementation in Task 6)

```rust
use crate::error::Result;

pub async fn migrate(_conn: &libsql::Connection, _dir: &str) -> Result<()> {
    // Stub — implemented in Task 6
    Ok(())
}
```

- [ ] **Step 4: Update src/ldb/mod.rs**

```rust
mod error;

mod config;
pub use config::{Config, JournalMode, SynchronousMode, TempStore};

mod database;
pub use database::Database;

mod connect;
pub use connect::connect;

pub(crate) mod migrate;
```

- [ ] **Step 5: Create tests/ldb_test.rs**

```rust
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
    let mut rows = db2.conn().query("SELECT name FROM sqlite_master WHERE type='table' AND name='test'", ()).await.unwrap();
    assert!(rows.next().await.unwrap().is_some());
}
```

- [ ] **Step 6: Add tempfile dev-dependency to Cargo.toml**

In `[dev-dependencies]`, add:

```toml
tempfile = "3"
```

- [ ] **Step 7: Run tests**

```bash
cargo test --features ldb -- ldb_test
```

Expected: 3 tests pass.

- [ ] **Step 8: Commit**

```bash
git add Cargo.toml src/ldb/ tests/ldb_test.rs
git commit -m "feat(ldb): add Database type and connect() with PRAGMA setup"
```

---

### Task 4: FromRow + ColumnMap

**Files:**
- Create: `src/ldb/from_row.rs`
- Modify: `src/ldb/mod.rs`
- Modify: `tests/ldb_test.rs`

- [ ] **Step 1: Create src/ldb/from_row.rs**

```rust
use std::collections::HashMap;

use crate::error::{Error, Result};

/// Trait for converting a libsql Row into a Rust struct.
/// Users implement this per struct, choosing positional or name-based access.
pub trait FromRow: Sized {
    fn from_row(row: &libsql::Row) -> Result<Self>;
}

/// Column name → index lookup. Built once per query, reused for all rows.
pub struct ColumnMap {
    map: HashMap<String, i32>,
}

impl ColumnMap {
    /// Build lookup from a row's column metadata.
    pub fn from_row(row: &libsql::Row) -> Self {
        let count = row.column_count();
        let mut map = HashMap::with_capacity(count as usize);
        for i in 0..count {
            if let Some(name) = row.column_name(i) {
                map.insert(name.to_string(), i);
            }
        }
        Self { map }
    }

    /// Get a typed value by column name.
    pub fn get<T: libsql::de::FromValue>(&self, row: &libsql::Row, name: &str) -> Result<T> {
        let idx = self.map.get(name).ok_or_else(|| {
            Error::internal(format!("column not found: {name}"))
        })?;
        row.get::<T>(*idx).map_err(Error::from)
    }
}
```

- [ ] **Step 2: Update src/ldb/mod.rs**

Add after existing exports:

```rust
mod from_row;
pub use from_row::{FromRow, ColumnMap};
```

- [ ] **Step 3: Add tests to tests/ldb_test.rs**

```rust
use modo::ldb::{FromRow, ColumnMap};
use modo::error::Result;

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

    let mut rows = conn.query("SELECT id, name, email FROM users WHERE id = 'u1'", ()).await.unwrap();
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
    let mut rows = conn.query("SELECT email, id, name FROM users WHERE id = 'u1'", ()).await.unwrap();
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

    let mut rows = conn.query("SELECT id FROM users WHERE id = 'u1'", ()).await.unwrap();
    let row = rows.next().await.unwrap().unwrap();
    let c = ColumnMap::from_row(&row);
    let result: modo::error::Result<String> = c.get(&row, "nonexistent");
    assert!(result.is_err());
}

// Test helpers (at bottom of file)
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
```

- [ ] **Step 4: Run tests**

```bash
cargo test --features ldb -- ldb_test
```

Expected: 6 tests pass (3 from Task 3 + 3 new).

- [ ] **Step 5: Commit**

```bash
git add src/ldb/ tests/ldb_test.rs
git commit -m "feat(ldb): add FromRow trait and ColumnMap for name-based access"
```

---

### Task 5: ConnExt trait

**Files:**
- Create: `src/ldb/conn.rs`
- Modify: `src/ldb/mod.rs`
- Modify: `tests/ldb_test.rs`

- [ ] **Step 1: Create src/ldb/conn.rs**

```rust
use libsql::params::IntoParams;

use crate::error::{Error, Result};

use super::from_row::FromRow;

/// Extension trait adding query helpers to libsql::Connection and libsql::Transaction.
pub trait ConnExt {
    fn query_raw(
        &self,
        sql: &str,
        params: impl IntoParams,
    ) -> impl std::future::Future<Output = std::result::Result<libsql::Rows, libsql::Error>> + Send;

    fn execute_raw(
        &self,
        sql: &str,
        params: impl IntoParams,
    ) -> impl std::future::Future<Output = std::result::Result<u64, libsql::Error>> + Send;
}

impl ConnExt for libsql::Connection {
    fn query_raw(
        &self,
        sql: &str,
        params: impl IntoParams,
    ) -> impl std::future::Future<Output = std::result::Result<libsql::Rows, libsql::Error>> + Send
    {
        self.query(sql, params)
    }

    fn execute_raw(
        &self,
        sql: &str,
        params: impl IntoParams,
    ) -> impl std::future::Future<Output = std::result::Result<u64, libsql::Error>> + Send
    {
        self.execute(sql, params)
    }
}

impl ConnExt for libsql::Transaction {
    fn query_raw(
        &self,
        sql: &str,
        params: impl IntoParams,
    ) -> impl std::future::Future<Output = std::result::Result<libsql::Rows, libsql::Error>> + Send
    {
        self.query(sql, params)
    }

    fn execute_raw(
        &self,
        sql: &str,
        params: impl IntoParams,
    ) -> impl std::future::Future<Output = std::result::Result<u64, libsql::Error>> + Send
    {
        self.execute(sql, params)
    }
}

/// High-level query helpers. Import this trait to use them.
pub trait ConnQueryExt: ConnExt {
    /// Fetch first row as T via FromRow. Returns Error::not_found if empty.
    fn query_one<T: FromRow>(
        &self,
        sql: &str,
        params: impl IntoParams,
    ) -> impl std::future::Future<Output = Result<T>> + Send {
        async move {
            let mut rows = self.query_raw(sql, params).await.map_err(Error::from)?;
            let row = rows
                .next()
                .await
                .map_err(Error::from)?
                .ok_or_else(|| Error::not_found("record not found"))?;
            T::from_row(&row)
        }
    }

    /// Fetch first row as T via FromRow. Returns None if empty.
    fn query_optional<T: FromRow>(
        &self,
        sql: &str,
        params: impl IntoParams,
    ) -> impl std::future::Future<Output = Result<Option<T>>> + Send {
        async move {
            let mut rows = self.query_raw(sql, params).await.map_err(Error::from)?;
            match rows.next().await.map_err(Error::from)? {
                Some(row) => Ok(Some(T::from_row(&row)?)),
                None => Ok(None),
            }
        }
    }

    /// Fetch all rows as Vec<T> via FromRow.
    fn query_all<T: FromRow>(
        &self,
        sql: &str,
        params: impl IntoParams,
    ) -> impl std::future::Future<Output = Result<Vec<T>>> + Send {
        async move {
            let mut rows = self.query_raw(sql, params).await.map_err(Error::from)?;
            let mut result = Vec::new();
            while let Some(row) = rows.next().await.map_err(Error::from)? {
                result.push(T::from_row(&row)?);
            }
            Ok(result)
        }
    }

    /// Fetch first row, map with closure. Returns Error::not_found if empty.
    fn query_one_map<T, F>(
        &self,
        sql: &str,
        params: impl IntoParams,
        f: F,
    ) -> impl std::future::Future<Output = Result<T>> + Send
    where
        F: Fn(&libsql::Row) -> Result<T> + Send,
    {
        async move {
            let mut rows = self.query_raw(sql, params).await.map_err(Error::from)?;
            let row = rows
                .next()
                .await
                .map_err(Error::from)?
                .ok_or_else(|| Error::not_found("record not found"))?;
            f(&row)
        }
    }

    /// Fetch first row, map with closure. Returns None if empty.
    fn query_optional_map<T, F>(
        &self,
        sql: &str,
        params: impl IntoParams,
        f: F,
    ) -> impl std::future::Future<Output = Result<Option<T>>> + Send
    where
        F: Fn(&libsql::Row) -> Result<T> + Send,
    {
        async move {
            let mut rows = self.query_raw(sql, params).await.map_err(Error::from)?;
            match rows.next().await.map_err(Error::from)? {
                Some(row) => Ok(Some(f(&row)?)),
                None => Ok(None),
            }
        }
    }

    /// Fetch all rows, map with closure.
    fn query_all_map<T, F>(
        &self,
        sql: &str,
        params: impl IntoParams,
        f: F,
    ) -> impl std::future::Future<Output = Result<Vec<T>>> + Send
    where
        F: Fn(&libsql::Row) -> Result<T> + Send,
    {
        async move {
            let mut rows = self.query_raw(sql, params).await.map_err(Error::from)?;
            let mut result = Vec::new();
            while let Some(row) = rows.next().await.map_err(Error::from)? {
                result.push(f(&row)?);
            }
            Ok(result)
        }
    }
}

// Blanket implementation: anything that implements ConnExt gets ConnQueryExt for free
impl<T: ConnExt> ConnQueryExt for T {}
```

- [ ] **Step 2: Update src/ldb/mod.rs**

Add:

```rust
mod conn;
pub use conn::{ConnExt, ConnQueryExt};
```

- [ ] **Step 3: Add tests to tests/ldb_test.rs**

```rust
use modo::ldb::ConnQueryExt;

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
```

- [ ] **Step 4: Run tests**

```bash
cargo test --features ldb -- ldb_test
```

Expected: 12 tests pass (6 previous + 6 new).

- [ ] **Step 5: Commit**

```bash
git add src/ldb/ tests/ldb_test.rs
git commit -m "feat(ldb): add ConnExt and ConnQueryExt extension traits"
```

---

### Task 6: Migration runner

**Files:**
- Modify: `src/ldb/migrate.rs` (replace stub)
- Modify: `tests/ldb_test.rs`

- [ ] **Step 1: Replace src/ldb/migrate.rs**

```rust
use crate::error::{Error, Result};

/// Run SQL migrations from a directory against a connection.
///
/// Reads `*.sql` files sorted by filename, tracks applied migrations
/// in a `_migrations` table with checksum verification.
pub async fn migrate(conn: &libsql::Connection, dir: &str) -> Result<()> {
    // Create tracking table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS _migrations (
            name TEXT PRIMARY KEY,
            checksum TEXT NOT NULL,
            applied_at TEXT NOT NULL DEFAULT (datetime('now'))
        )",
        (),
    )
    .await
    .map_err(Error::from)?;

    // Read and sort migration files
    let dir_path = std::path::Path::new(dir);
    if !dir_path.exists() {
        return Ok(()); // No migrations directory — nothing to do
    }

    let mut files: Vec<std::fs::DirEntry> = std::fs::read_dir(dir_path)
        .map_err(|e| Error::internal(format!("failed to read migrations directory: {dir}")).chain(e))?
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry
                .path()
                .extension()
                .is_some_and(|ext| ext == "sql")
        })
        .collect();
    files.sort_by_key(|e| e.file_name());

    for entry in files {
        let name = entry.file_name().to_string_lossy().to_string();
        let sql = std::fs::read_to_string(entry.path()).map_err(|e| {
            Error::internal(format!("failed to read migration file: {name}")).chain(e)
        })?;
        let checksum = fnv1a_hex(sql.as_bytes());

        // Check if already applied
        let mut rows = conn
            .query(
                "SELECT checksum FROM _migrations WHERE name = ?1",
                libsql::params![name.clone()],
            )
            .await
            .map_err(Error::from)?;

        if let Some(row) = rows.next().await.map_err(Error::from)? {
            let existing: String = row.get(0).map_err(Error::from)?;
            if existing != checksum {
                return Err(Error::internal(format!(
                    "migration '{name}' checksum mismatch — file was modified after applying (expected {existing}, got {checksum})"
                )));
            }
            continue; // Already applied
        }

        // Apply migration
        conn.execute_batch(&sql).await.map_err(|e| {
            Error::internal(format!("failed to apply migration '{name}'")).chain(e)
        })?;

        conn.execute(
            "INSERT INTO _migrations (name, checksum) VALUES (?1, ?2)",
            libsql::params![name.clone(), checksum],
        )
        .await
        .map_err(Error::from)?;
    }

    Ok(())
}

/// FNV-1a hash, deterministic and stable across Rust versions.
fn fnv1a_hex(data: &[u8]) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for &byte in data {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{:016x}", hash)
}
```

- [ ] **Step 2: Update src/ldb/mod.rs — make migrate public**

Change:

```rust
pub(crate) mod migrate;
```

to:

```rust
mod migrate;
pub use migrate::migrate;
```

- [ ] **Step 3: Add tests**

Create `tests/fixtures/ldb_migrations/001_create_users.sql`:

```sql
CREATE TABLE users (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    email TEXT NOT NULL
);
```

Create `tests/fixtures/ldb_migrations/002_add_bio.sql`:

```sql
ALTER TABLE users ADD COLUMN bio TEXT NOT NULL DEFAULT '';
```

Add to `tests/ldb_test.rs`:

```rust
#[tokio::test]
async fn migrate_applies_sql_files_in_order() {
    let db = test_db().await;
    let conn = db.conn();

    ldb::migrate(conn, "tests/fixtures/ldb_migrations").await.unwrap();

    // Verify table was created with bio column
    conn.execute(
        "INSERT INTO users (id, name, email, bio) VALUES ('u1', 'Alice', 'a@t.com', 'hello')",
        (),
    )
    .await
    .unwrap();

    let mut rows = conn.query("SELECT bio FROM users WHERE id = 'u1'", ()).await.unwrap();
    let row = rows.next().await.unwrap().unwrap();
    let bio: String = row.get(0).unwrap();
    assert_eq!(bio, "hello");
}

#[tokio::test]
async fn migrate_is_idempotent() {
    let db = test_db().await;
    let conn = db.conn();

    ldb::migrate(conn, "tests/fixtures/ldb_migrations").await.unwrap();
    // Run again — should not error
    ldb::migrate(conn, "tests/fixtures/ldb_migrations").await.unwrap();

    // Verify _migrations table has 2 entries
    let mut rows = conn.query("SELECT COUNT(*) FROM _migrations", ()).await.unwrap();
    let row = rows.next().await.unwrap().unwrap();
    let count: i64 = row.get(0).unwrap();
    assert_eq!(count, 2);
}

#[tokio::test]
async fn migrate_nonexistent_dir_is_ok() {
    let db = test_db().await;
    let conn = db.conn();

    // Should not error — just skip
    ldb::migrate(conn, "tests/fixtures/nonexistent").await.unwrap();
}
```

- [ ] **Step 4: Run tests**

```bash
cargo test --features ldb -- ldb_test
```

Expected: 15 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/ldb/ tests/fixtures/ldb_migrations/ tests/ldb_test.rs
git commit -m "feat(ldb): add migration runner with checksum verification"
```

---

### Task 7: Managed shutdown

**Files:**
- Create: `src/ldb/managed.rs`
- Modify: `src/ldb/mod.rs`

- [ ] **Step 1: Create src/ldb/managed.rs**

```rust
use crate::error::Result;
use crate::runtime::Task;

use super::database::Database;

/// Wrapper for graceful shutdown integration with `modo::run!()`.
pub struct ManagedDatabase(Database);

impl Task for ManagedDatabase {
    async fn shutdown(self) -> Result<()> {
        // Dropping Database drops the Arc. When the last reference is dropped,
        // Inner is dropped, which drops libsql::Connection and libsql::Database.
        // libsql handles cleanup internally.
        drop(self.0);
        Ok(())
    }
}

/// Wrap a Database for use with `modo::run!()`.
pub fn managed(db: Database) -> ManagedDatabase {
    ManagedDatabase(db)
}
```

- [ ] **Step 2: Update src/ldb/mod.rs**

Add:

```rust
mod managed;
pub use managed::{managed, ManagedDatabase};
```

- [ ] **Step 3: Verify it compiles**

```bash
cargo check --features ldb
```

- [ ] **Step 4: Commit**

```bash
git add src/ldb/
git commit -m "feat(ldb): add ManagedDatabase for graceful shutdown"
```

---

### Task 8: Pagination types and extractors

**Files:**
- Create: `src/ldb/page.rs`
- Modify: `src/ldb/mod.rs`
- Modify: `tests/ldb_test.rs`

- [ ] **Step 1: Create src/ldb/page.rs**

```rust
use serde::{Deserialize, Serialize};

/// Pagination configuration. Add to app state for extractor clamping.
#[derive(Debug, Clone)]
pub struct PaginationConfig {
    pub default_per_page: i64,
    pub max_per_page: i64,
}

impl Default for PaginationConfig {
    fn default() -> Self {
        Self {
            default_per_page: 20,
            max_per_page: 100,
        }
    }
}

/// Offset-based page response.
#[derive(Debug, Serialize)]
pub struct Page<T: Serialize> {
    pub items: Vec<T>,
    pub total: i64,
    pub page: i64,
    pub per_page: i64,
    pub total_pages: i64,
    pub has_next: bool,
    pub has_prev: bool,
}

impl<T: Serialize> Page<T> {
    pub fn new(items: Vec<T>, total: i64, page: i64, per_page: i64) -> Self {
        let total_pages = if total == 0 {
            0
        } else {
            (total + per_page - 1) / per_page
        };
        Self {
            items,
            total,
            page,
            per_page,
            total_pages,
            has_next: page < total_pages,
            has_prev: page > 1,
        }
    }
}

/// Cursor-based page response.
#[derive(Debug, Serialize)]
pub struct CursorPage<T: Serialize> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
    pub per_page: i64,
}

/// Offset pagination request. Extracted from query string: `?page=N&per_page=N`.
#[derive(Debug, Clone, Deserialize)]
pub struct PageRequest {
    #[serde(default = "one")]
    pub page: i64,
    #[serde(default)]
    pub per_page: i64,
}

impl PageRequest {
    /// Clamp values using config.
    pub fn clamp(&mut self, config: &PaginationConfig) {
        if self.page < 1 {
            self.page = 1;
        }
        if self.per_page < 1 {
            self.per_page = config.default_per_page;
        }
        if self.per_page > config.max_per_page {
            self.per_page = config.max_per_page;
        }
    }

    pub fn offset(&self) -> i64 {
        (self.page - 1) * self.per_page
    }
}

/// Cursor pagination request. Extracted from query string: `?after=<cursor>&per_page=N`.
#[derive(Debug, Clone, Deserialize)]
pub struct CursorRequest {
    #[serde(default)]
    pub after: Option<String>,
    #[serde(default)]
    pub per_page: i64,
}

impl CursorRequest {
    pub fn clamp(&mut self, config: &PaginationConfig) {
        if self.per_page < 1 {
            self.per_page = config.default_per_page;
        }
        if self.per_page > config.max_per_page {
            self.per_page = config.max_per_page;
        }
    }
}

fn one() -> i64 {
    1
}

// axum extractors
#[axum::async_trait]
impl<S: Send + Sync> axum::extract::FromRequestParts<S> for PageRequest {
    type Rejection = crate::error::Error;

    async fn from_request_parts(
        parts: &mut http::request::Parts,
        state: &S,
    ) -> std::result::Result<Self, Self::Rejection> {
        let axum::extract::Query(mut req) =
            axum::extract::Query::<PageRequest>::from_request_parts(parts, state)
                .await
                .map_err(|e| crate::error::Error::bad_request(e.to_string()))?;

        let config = parts
            .extensions
            .get::<PaginationConfig>()
            .cloned()
            .unwrap_or_default();
        req.clamp(&config);
        Ok(req)
    }
}

#[axum::async_trait]
impl<S: Send + Sync> axum::extract::FromRequestParts<S> for CursorRequest {
    type Rejection = crate::error::Error;

    async fn from_request_parts(
        parts: &mut http::request::Parts,
        state: &S,
    ) -> std::result::Result<Self, Self::Rejection> {
        let axum::extract::Query(mut req) =
            axum::extract::Query::<CursorRequest>::from_request_parts(parts, state)
                .await
                .map_err(|e| crate::error::Error::bad_request(e.to_string()))?;

        let config = parts
            .extensions
            .get::<PaginationConfig>()
            .cloned()
            .unwrap_or_default();
        req.clamp(&config);
        Ok(req)
    }
}
```

- [ ] **Step 2: Update src/ldb/mod.rs**

Add:

```rust
mod page;
pub use page::{
    CursorPage, CursorRequest, Page, PageRequest, PaginationConfig,
};
```

- [ ] **Step 3: Add tests**

```rust
#[tokio::test]
async fn page_request_clamp() {
    let config = ldb::PaginationConfig {
        default_per_page: 20,
        max_per_page: 50,
    };

    let mut req = ldb::PageRequest { page: 0, per_page: 0 };
    req.clamp(&config);
    assert_eq!(req.page, 1);
    assert_eq!(req.per_page, 20);

    let mut req = ldb::PageRequest { page: 3, per_page: 200 };
    req.clamp(&config);
    assert_eq!(req.page, 3);
    assert_eq!(req.per_page, 50);
}

#[tokio::test]
async fn page_new_calculates_fields() {
    let page: ldb::Page<String> = ldb::Page::new(
        vec!["a".into(), "b".into()],
        5,  // total
        2,  // page
        2,  // per_page
    );
    assert_eq!(page.total_pages, 3);
    assert!(page.has_next);
    assert!(page.has_prev);
}

#[tokio::test]
async fn cursor_request_clamp() {
    let config = ldb::PaginationConfig::default();

    let mut req = ldb::CursorRequest { after: None, per_page: 0 };
    req.clamp(&config);
    assert_eq!(req.per_page, 20);

    let mut req = ldb::CursorRequest { after: Some("abc".into()), per_page: 999 };
    req.clamp(&config);
    assert_eq!(req.per_page, 100);
}
```

- [ ] **Step 4: Run tests**

```bash
cargo test --features ldb -- ldb_test
```

Expected: 18 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/ldb/ tests/ldb_test.rs
git commit -m "feat(ldb): add pagination types and extractors"
```

---

### Task 9: Filter DSL

**Files:**
- Create: `src/ldb/filter.rs`
- Modify: `src/ldb/mod.rs`
- Modify: `tests/ldb_test.rs`

- [ ] **Step 1: Create src/ldb/filter.rs**

```rust
use std::collections::HashMap;

use crate::error::{Error, Result};

/// Defines allowed filter fields and sort fields for an endpoint.
pub struct FilterSchema {
    fields: Vec<(String, FieldType)>,
    sort_fields: Vec<String>,
}

/// Column type for validation.
#[derive(Debug, Clone, Copy)]
pub enum FieldType {
    Text,
    Int,
    Float,
    Date,
    Bool,
}

impl FilterSchema {
    pub fn new() -> Self {
        Self {
            fields: Vec::new(),
            sort_fields: Vec::new(),
        }
    }

    pub fn field(mut self, name: &str, typ: FieldType) -> Self {
        self.fields.push((name.to_string(), typ));
        self
    }

    pub fn sort_fields(mut self, fields: &[&str]) -> Self {
        self.sort_fields = fields.iter().map(|s| s.to_string()).collect();
        self
    }

    fn field_type(&self, name: &str) -> Option<FieldType> {
        self.fields.iter().find(|(n, _)| n == name).map(|(_, t)| *t)
    }

    fn is_sort_field(&self, name: &str) -> bool {
        self.sort_fields.iter().any(|f| f == name)
    }
}

/// Parsed operator from query string.
#[derive(Debug, Clone)]
enum Operator {
    Eq,
    Ne,
    Gt,
    Gte,
    Lt,
    Lte,
    Like,
    IsNull(bool),
    In,
}

/// A single parsed filter condition.
#[derive(Debug, Clone)]
struct FilterCondition {
    column: String,
    operator: Operator,
    values: Vec<String>,
}

/// Raw parsed filter from query string. Must be validated before use.
pub struct Filter {
    conditions: Vec<FilterCondition>,
    sort: Option<String>,
}

/// Validated filter — safe to use in SQL generation.
pub struct ValidatedFilter {
    pub(crate) clauses: Vec<String>,
    pub(crate) params: Vec<libsql::Value>,
    pub(crate) sort_clause: Option<String>,
}

impl ValidatedFilter {
    pub fn is_empty(&self) -> bool {
        self.clauses.is_empty()
    }
}

impl Filter {
    /// Parse filter conditions from a query string map.
    pub fn from_query_params(params: &HashMap<String, Vec<String>>) -> Self {
        let mut conditions: HashMap<String, FilterCondition> = HashMap::new();
        let mut sort = None;

        for (key, values) in params {
            if key == "sort" {
                if let Some(v) = values.first() {
                    sort = Some(v.clone());
                }
                continue;
            }

            // Skip pagination params
            if key == "page" || key == "per_page" || key == "after" {
                continue;
            }

            // Parse operator from key: "field.op" or just "field"
            let (column, op) = if let Some(dot_pos) = key.rfind('.') {
                let col = &key[..dot_pos];
                let op_str = &key[dot_pos + 1..];
                let op = match op_str {
                    "ne" => Operator::Ne,
                    "gt" => Operator::Gt,
                    "gte" => Operator::Gte,
                    "lt" => Operator::Lt,
                    "lte" => Operator::Lte,
                    "like" => Operator::Like,
                    "null" => {
                        let is_null = values.first().map(|v| v == "true").unwrap_or(true);
                        Operator::IsNull(is_null)
                    }
                    _ => continue, // Unknown operator — skip
                };
                (col.to_string(), op)
            } else {
                // No operator — Eq (single value) or In (multiple values)
                if values.len() > 1 {
                    (key.clone(), Operator::In)
                } else {
                    (key.clone(), Operator::Eq)
                }
            };

            conditions.insert(
                format!("{key}"),
                FilterCondition {
                    column,
                    operator: op,
                    values: values.clone(),
                },
            );
        }

        Self {
            conditions: conditions.into_values().collect(),
            sort,
        }
    }

    /// Validate against a schema. Unknown columns are silently ignored.
    /// Type mismatches return a 400 error.
    pub fn validate(self, schema: &FilterSchema) -> Result<ValidatedFilter> {
        let mut clauses = Vec::new();
        let mut params: Vec<libsql::Value> = Vec::new();

        for cond in &self.conditions {
            let Some(field_type) = schema.field_type(&cond.column) else {
                continue; // Unknown column — silently ignore
            };

            match &cond.operator {
                Operator::IsNull(is_null) => {
                    if *is_null {
                        clauses.push(format!("{} IS NULL", cond.column));
                    } else {
                        clauses.push(format!("{} IS NOT NULL", cond.column));
                    }
                }
                Operator::In => {
                    let placeholders: Vec<String> = cond
                        .values
                        .iter()
                        .map(|_| "?".to_string())
                        .collect();
                    clauses.push(format!(
                        "{} IN ({})",
                        cond.column,
                        placeholders.join(", ")
                    ));
                    for val in &cond.values {
                        params.push(convert_value(val, field_type)?);
                    }
                }
                op => {
                    let sql_op = match op {
                        Operator::Eq => "=",
                        Operator::Ne => "!=",
                        Operator::Gt => ">",
                        Operator::Gte => ">=",
                        Operator::Lt => "<",
                        Operator::Lte => "<=",
                        Operator::Like => "LIKE",
                        _ => unreachable!(),
                    };
                    clauses.push(format!("{} {} ?", cond.column, sql_op));
                    let val = cond.values.first().ok_or_else(|| {
                        Error::bad_request(format!("missing value for filter '{}'", cond.column))
                    })?;
                    params.push(convert_value(val, field_type)?);
                }
            }
        }

        // Validate sort
        let sort_clause = self.sort.and_then(|s| {
            let (field, desc) = if let Some(stripped) = s.strip_prefix('-') {
                (stripped, true)
            } else {
                (s.as_str(), false)
            };
            if schema.is_sort_field(field) {
                let direction = if desc { "DESC" } else { "ASC" };
                Some(format!("{field} {direction}"))
            } else {
                None // Unknown sort field — ignore
            }
        });

        Ok(ValidatedFilter {
            clauses,
            params,
            sort_clause,
        })
    }
}

fn convert_value(val: &str, field_type: FieldType) -> Result<libsql::Value> {
    match field_type {
        FieldType::Text | FieldType::Date => Ok(libsql::Value::from(val.to_string())),
        FieldType::Int => {
            let n: i64 = val.parse().map_err(|_| {
                Error::bad_request(format!("invalid integer value: '{val}'"))
            })?;
            Ok(libsql::Value::from(n))
        }
        FieldType::Float => {
            let n: f64 = val.parse().map_err(|_| {
                Error::bad_request(format!("invalid float value: '{val}'"))
            })?;
            Ok(libsql::Value::from(n))
        }
        FieldType::Bool => {
            let b = matches!(val, "true" | "1" | "yes");
            Ok(libsql::Value::from(b as i32))
        }
    }
}

// axum extractor
#[axum::async_trait]
impl<S: Send + Sync> axum::extract::FromRequestParts<S> for Filter {
    type Rejection = crate::error::Error;

    async fn from_request_parts(
        parts: &mut http::request::Parts,
        _state: &S,
    ) -> std::result::Result<Self, Self::Rejection> {
        let uri = &parts.uri;
        let query = uri.query().unwrap_or("");

        // Parse query string into HashMap<String, Vec<String>>
        let mut params: HashMap<String, Vec<String>> = HashMap::new();
        for pair in query.split('&') {
            if pair.is_empty() {
                continue;
            }
            let (key, value) = match pair.split_once('=') {
                Some((k, v)) => (k, v),
                None => (pair, ""),
            };
            let key = urlencoding::decode(key)
                .unwrap_or_else(|_| key.into())
                .to_string();
            let value = urlencoding::decode(value)
                .unwrap_or_else(|_| value.into())
                .to_string();
            params.entry(key).or_default().push(value);
        }

        Ok(Filter::from_query_params(&params))
    }
}
```

- [ ] **Step 2: Add urlencoding dependency to Cargo.toml**

In `[dependencies]`, add:

```toml
urlencoding = { version = "2", optional = true }
```

Update the `ldb` feature:

```toml
ldb = ["dep:libsql", "dep:urlencoding"]
```

- [ ] **Step 3: Update src/ldb/mod.rs**

Add:

```rust
mod filter;
pub use filter::{FieldType, Filter, FilterSchema, ValidatedFilter};
```

- [ ] **Step 4: Add tests**

```rust
use std::collections::HashMap;
use modo::ldb::{Filter, FilterSchema, FieldType};

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
```

- [ ] **Step 5: Run tests**

```bash
cargo test --features ldb -- ldb_test
```

Expected: 26 tests pass.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml src/ldb/ tests/ldb_test.rs
git commit -m "feat(ldb): add Filter DSL with schema validation and extraction"
```

---

### Task 10: SelectBuilder

**Files:**
- Create: `src/ldb/select.rs`
- Modify: `src/ldb/conn.rs`
- Modify: `src/ldb/mod.rs`
- Modify: `tests/ldb_test.rs`

- [ ] **Step 1: Create src/ldb/select.rs**

```rust
use crate::error::{Error, Result};

use super::conn::ConnExt;
use super::filter::ValidatedFilter;
use super::from_row::FromRow;
use super::page::{CursorPage, CursorRequest, Page, PageRequest};

/// Composable query builder for filter + pagination.
pub struct SelectBuilder<'a, C: ConnExt> {
    conn: &'a C,
    base_sql: String,
    filter: Option<ValidatedFilter>,
    order_by: Option<String>,
}

impl<'a, C: ConnExt> SelectBuilder<'a, C> {
    pub(crate) fn new(conn: &'a C, sql: &str) -> Self {
        Self {
            conn,
            base_sql: sql.to_string(),
            filter: None,
            order_by: None,
        }
    }

    /// Apply a validated filter (WHERE clauses).
    pub fn filter(mut self, filter: ValidatedFilter) -> Self {
        self.filter = Some(filter);
        self
    }

    /// Set ORDER BY clause. This is raw SQL — not user input.
    /// If a filter has a sort_clause, it takes precedence over this.
    pub fn order_by(mut self, order: &str) -> Self {
        self.order_by = Some(order.to_string());
        self
    }

    /// Build WHERE clause and params from filter.
    fn build_where(&self) -> (String, Vec<libsql::Value>) {
        match &self.filter {
            Some(f) if !f.clauses.is_empty() => {
                let where_sql = format!(" WHERE {}", f.clauses.join(" AND "));
                (where_sql, f.params.clone())
            }
            _ => (String::new(), Vec::new()),
        }
    }

    /// Resolve ORDER BY — filter sort takes precedence, then explicit order_by.
    fn resolve_order(&self) -> Option<String> {
        self.filter
            .as_ref()
            .and_then(|f| f.sort_clause.clone())
            .or_else(|| self.order_by.clone())
    }

    /// Execute with offset pagination. Returns Page<T>.
    pub async fn page<T: FromRow + serde::Serialize>(
        self,
        req: PageRequest,
    ) -> Result<Page<T>> {
        let (where_sql, mut params) = self.build_where();
        let order = self.resolve_order();

        // Count query
        let count_sql = format!(
            "SELECT COUNT(*) FROM ({}{}) AS _count",
            self.base_sql, where_sql
        );
        let mut rows = self
            .conn
            .query_raw(&count_sql, params.clone())
            .await
            .map_err(Error::from)?;
        let total: i64 = rows
            .next()
            .await
            .map_err(Error::from)?
            .ok_or_else(|| Error::internal("count query returned no rows"))?
            .get(0)
            .map_err(Error::from)?;

        // Data query
        let order_sql = order
            .map(|o| format!(" ORDER BY {o}"))
            .unwrap_or_default();
        let data_sql = format!(
            "{}{}{} LIMIT ? OFFSET ?",
            self.base_sql, where_sql, order_sql
        );
        params.push(libsql::Value::from(req.per_page));
        params.push(libsql::Value::from(req.offset()));

        let mut rows = self
            .conn
            .query_raw(&data_sql, params)
            .await
            .map_err(Error::from)?;
        let mut items = Vec::new();
        while let Some(row) = rows.next().await.map_err(Error::from)? {
            items.push(T::from_row(&row)?);
        }

        Ok(Page::new(items, total, req.page, req.per_page))
    }

    /// Execute with cursor pagination. Returns CursorPage<T>.
    ///
    /// Assumes the first column in the SELECT is the cursor ID (e.g., ULID).
    pub async fn cursor<T: FromRow + serde::Serialize>(
        self,
        req: CursorRequest,
    ) -> Result<CursorPage<T>> {
        let (where_sql, mut params) = self.build_where();

        // Add cursor condition
        let cursor_condition = if let Some(ref after) = req.after {
            params.push(libsql::Value::from(after.clone()));
            if where_sql.is_empty() {
                " WHERE id > ?".to_string()
            } else {
                " AND id > ?".to_string()
            }
        } else {
            String::new()
        };

        // Fetch one extra to determine has_more
        let limit = req.per_page + 1;
        let sql = format!(
            "{}{}{} ORDER BY id ASC LIMIT ?",
            self.base_sql, where_sql, cursor_condition
        );
        params.push(libsql::Value::from(limit));

        let mut rows = self
            .conn
            .query_raw(&sql, params)
            .await
            .map_err(Error::from)?;

        // Track IDs alongside items for cursor extraction
        let mut items = Vec::new();
        let mut ids: Vec<Option<String>> = Vec::new();
        while let Some(row) = rows.next().await.map_err(Error::from)? {
            ids.push(row.get::<String>(0).ok());
            items.push(T::from_row(&row)?);
        }

        let has_more = items.len() as i64 > req.per_page;
        if has_more {
            items.pop();
            ids.pop();
        }

        let next_cursor = if has_more {
            ids.last().cloned().flatten()
        } else {
            None
        };

        Ok(CursorPage {
            items,
            next_cursor,
            has_more,
            per_page: req.per_page,
        })
    }

    /// Execute without pagination. Returns Vec<T>.
    pub async fn fetch_all<T: FromRow>(self) -> Result<Vec<T>> {
        let (where_sql, params) = self.build_where();
        let order = self.resolve_order();
        let order_sql = order
            .map(|o| format!(" ORDER BY {o}"))
            .unwrap_or_default();
        let sql = format!("{}{}{}", self.base_sql, where_sql, order_sql);

        let mut rows = self
            .conn
            .query_raw(&sql, params)
            .await
            .map_err(Error::from)?;
        let mut items = Vec::new();
        while let Some(row) = rows.next().await.map_err(Error::from)? {
            items.push(T::from_row(&row)?);
        }
        Ok(items)
    }

    /// Execute without pagination. Returns first row.
    pub async fn fetch_one<T: FromRow>(self) -> Result<T> {
        let (where_sql, params) = self.build_where();
        let sql = format!("{}{} LIMIT 1", self.base_sql, where_sql);

        let mut rows = self
            .conn
            .query_raw(&sql, params)
            .await
            .map_err(Error::from)?;
        let row = rows
            .next()
            .await
            .map_err(Error::from)?
            .ok_or_else(|| Error::not_found("record not found"))?;
        T::from_row(&row)
    }

    /// Execute without pagination. Returns Option<T>.
    pub async fn fetch_optional<T: FromRow>(self) -> Result<Option<T>> {
        let (where_sql, params) = self.build_where();
        let sql = format!("{}{} LIMIT 1", self.base_sql, where_sql);

        let mut rows = self
            .conn
            .query_raw(&sql, params)
            .await
            .map_err(Error::from)?;
        match rows.next().await.map_err(Error::from)? {
            Some(row) => Ok(Some(T::from_row(&row)?)),
            None => Ok(None),
        }
    }
}
```

- [ ] **Step 2: Add select() method to ConnExt in src/ldb/conn.rs**

Add to the `ConnExt` trait definition:

```rust
fn select<'a>(&'a self, sql: &str) -> super::select::SelectBuilder<'a, Self>
where
    Self: Sized,
{
    super::select::SelectBuilder::new(self, sql)
}
```

- [ ] **Step 3: Update src/ldb/mod.rs**

Add:

```rust
mod select;
pub use select::SelectBuilder;
```

- [ ] **Step 4: Add tests**

```rust
use modo::ldb::{Filter, FilterSchema, FieldType, PageRequest};

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
    let filter = Filter::from_query_params(&params).validate(&schema).unwrap();

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
    let filter = Filter::from_query_params(&params).validate(&schema).unwrap();

    let page_req = PageRequest { page: 1, per_page: 10 };
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
    let filter = Filter::from_query_params(&params).validate(&schema).unwrap();

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

    let page_req = PageRequest { page: 2, per_page: 20 };
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
```

- [ ] **Step 5: Run tests**

```bash
cargo test --features ldb -- ldb_test
```

Expected: 30 tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/ldb/ tests/ldb_test.rs
git commit -m "feat(ldb): add SelectBuilder with filter and pagination composition"
```

---

### Task 11: Wire into lib.rs + final re-exports

**Files:**
- Modify: `src/lib.rs`
- Modify: `src/ldb/mod.rs`

- [ ] **Step 1: Finalize src/ldb/mod.rs with all exports**

Replace the entire file:

```rust
mod error;

mod config;
pub use config::{Config, JournalMode, SynchronousMode, TempStore};

mod database;
pub use database::Database;

mod connect;
pub use connect::connect;

mod from_row;
pub use from_row::{ColumnMap, FromRow};

mod conn;
pub use conn::{ConnExt, ConnQueryExt};

mod migrate;
pub use migrate::migrate;

mod managed;
pub use managed::{managed, ManagedDatabase};

mod page;
pub use page::{CursorPage, CursorRequest, Page, PageRequest, PaginationConfig};

mod filter;
pub use filter::{FieldType, Filter, FilterSchema, ValidatedFilter};

mod select;
pub use select::SelectBuilder;

// Re-export libsql for direct access
pub use libsql;
```

- [ ] **Step 2: Add re-exports to src/lib.rs**

In the public re-export section of `src/lib.rs`, add the ldb re-exports (feature-gated):

```rust
#[cfg(feature = "ldb")]
pub use ldb::{
    // Core
    connect as ldb_connect, Config as LdbConfig, Database as LdbDatabase,
    // Re-export the module itself for qualified access
};
```

Actually — since `ldb` is already a `pub mod`, users access it as `modo::ldb::*`. No need for flat re-exports. Just ensure the module declaration exists (done in Task 1).

- [ ] **Step 3: Run full test suite**

```bash
cargo test --features ldb
```

Expected: All ldb tests pass, and existing tests are unaffected.

- [ ] **Step 4: Run clippy**

```bash
cargo clippy --features ldb --tests -- -D warnings
```

Fix any warnings.

- [ ] **Step 5: Commit**

```bash
git add src/
git commit -m "feat(ldb): finalize module exports and lib.rs integration"
```

---

## Summary

| Task | What | Files |
|------|------|-------|
| 1 | Foundation — Cargo.toml, mod.rs, error.rs | 3 |
| 2 | Config — struct, enums, defaults | 1 |
| 3 | Database + Connect — core types, PRAGMA setup | 3 + test |
| 4 | FromRow + ColumnMap | 1 + test |
| 5 | ConnExt — query helpers | 1 + test |
| 6 | Migration runner | 1 + fixtures + test |
| 7 | Managed shutdown | 1 |
| 8 | Pagination types + extractors | 1 + test |
| 9 | Filter DSL | 1 + test |
| 10 | SelectBuilder | 1 + test |
| 11 | Wire into lib.rs | 2 |
