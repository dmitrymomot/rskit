# ldb Module — libsql Database Layer for modo

**Date**: 2026-03-28
**Goal**: New database module for modo based on libsql, replacing the sqlx-based `db` and `page` modules with a single-connection architecture optimized for heavy read/write load, with query helpers, safe filter DSL, pagination, and migration runner.

## Context

Benchmarks confirmed that libsql with a single connection outperforms sqlx with pooled connections by 2-4x across all scenarios. The current `db` module is built on sqlx with read/write pool splitting. This new `ldb` module provides a clean replacement based on libsql, living alongside the existing `db` and `page` modules during transition.

Migration of existing internal consumers (session, job, etc.) to the new module is out of scope.

## Design Principles

- No backward compatibility — clean slate
- Clean code over following existing patterns
- No tech debt — every declared API fully implemented
- Breaking changes allowed
- Extension trait on libsql types, not wrapper types
- Raw libsql always accessible — helpers are opt-in convenience

## Module Structure

```
src/ldb/
├── mod.rs          # pub exports
├── config.rs       # Config struct, YAML deserialization, defaults
├── connect.rs      # connect() → Database, PRAGMA setup, auto-migration
├── database.rs     # Database (Arc<Inner>, holds single connection)
├── conn.rs         # ConnExt trait (query_one, query_all, query_optional, etc.)
├── from_row.rs     # FromRow trait, ColumnMap helper
├── select.rs       # SelectBuilder (filter + pagination + fetch)
├── filter.rs       # FilterSchema, Filter, query string parsing, extraction
├── page.rs         # Page<T>, CursorPage<T>, PageRequest, CursorRequest, PaginationConfig
├── migrate.rs      # migration runner (_migrations table, checksum verification)
├── error.rs        # libsql::Error → modo::Error conversion
└── managed.rs      # Task impl for graceful shutdown
```

## Config

```rust
#[derive(Deserialize)]
pub struct Config {
    /// Database file path. Default: "data/app.db"
    pub path: String,
    /// Migration directory. If set, migrations run on connect. Default: None
    #[serde(default)]
    pub migrations: Option<String>,
    /// Busy timeout in milliseconds. Default: 5000
    #[serde(default = "defaults::busy_timeout")]
    pub busy_timeout: u64,
    /// Cache size in KB (converted to PRAGMA cache_size = -N). Default: 16384 (16MB)
    #[serde(default = "defaults::cache_size")]
    pub cache_size: i64,
    /// Memory-mapped I/O size in bytes. Default: 268435456 (256MB)
    #[serde(default = "defaults::mmap_size")]
    pub mmap_size: u64,
    /// Journal mode. Default: WAL
    #[serde(default = "defaults::journal_mode")]
    pub journal_mode: JournalMode,
    /// Synchronous mode. Default: Normal
    #[serde(default = "defaults::synchronous")]
    pub synchronous: SynchronousMode,
    /// Foreign key enforcement. Default: true
    #[serde(default = "defaults::foreign_keys")]
    pub foreign_keys: bool,
    /// Temp store location. Default: Memory
    #[serde(default = "defaults::temp_store")]
    pub temp_store: TempStore,
}
```

YAML example:

```yaml
database:
  path: ${DB_PATH:data/app.db}
  migrations: migrations
  busy_timeout: 5000
  cache_size: 16384
  mmap_size: 268435456
  journal_mode: wal
  synchronous: normal
  foreign_keys: true
  temp_store: memory
```

All fields optional with sensible defaults. Multiple databases supported by having multiple config sections:

```yaml
database:
  path: ${DB_PATH:data/app.db}
  migrations: migrations

jobs_database:
  path: ${JOBS_DB_PATH:data/jobs.db}
  migrations: migrations/jobs
```

Each database is created independently via `ldb::connect()`.

### PRAGMA enums

```rust
pub enum JournalMode { Wal, Delete, Truncate, Memory, Off }
pub enum SynchronousMode { Off, Normal, Full, Extra }
pub enum TempStore { Default, File, Memory }
```

Serialized/deserialized as lowercase strings in YAML.

## Database

```rust
/// Single-connection database handle. Clone-able (Arc internally).
#[derive(Clone)]
pub struct Database {
    inner: Arc<Inner>,
}

struct Inner {
    db: libsql::Database,
    conn: libsql::Connection,
}

impl Database {
    /// Returns a reference to the underlying libsql connection.
    pub fn conn(&self) -> &libsql::Connection {
        &self.inner.conn
    }
}
```

### connect()

```rust
/// Open a database, apply PRAGMAs, run migrations if configured.
pub async fn connect(config: &Config) -> Result<Database>;
```

Steps:
1. Create parent directories for `config.path` if needed
2. `libsql::Builder::new_local(&config.path).build().await?`
3. `db.connect()?` — single connection
4. Apply PRAGMAs via `conn.query()` (PRAGMAs return rows in libsql)
5. If `config.migrations` is set, run `migrate(&conn, &path).await?`
6. Return `Database { inner: Arc::new(Inner { db, conn }) }`

### Usage

```rust
// main.rs
let db = ldb::connect(&config.database).await?;
let jdb = ldb::connect(&config.jobs_database).await?;

registry.add(db.clone());  // for handlers via Service<Database>

let worker = Worker::builder(&config.job, &registry)
    .database(jdb)          // jobs module uses its own db
    .start()
    .await;

modo::run!(server, ldb::managed(db)).await;
```

## ConnExt Trait

Extension trait on `libsql::Connection` and `libsql::Transaction`. Adds query helpers without wrapping.

```rust
pub trait ConnExt {
    /// Fetch first row as T via FromRow. Error::not_found if empty.
    async fn query_one<T: FromRow>(&self, sql: &str, params: impl IntoParams) -> Result<T>;

    /// Fetch first row as T via FromRow. None if empty.
    async fn query_optional<T: FromRow>(&self, sql: &str, params: impl IntoParams) -> Result<Option<T>>;

    /// Fetch all rows as Vec<T> via FromRow.
    async fn query_all<T: FromRow>(&self, sql: &str, params: impl IntoParams) -> Result<Vec<T>>;

    /// Fetch first row, map with closure. Error::not_found if empty.
    async fn query_one_map<T>(
        &self, sql: &str, params: impl IntoParams,
        f: impl Fn(&libsql::Row) -> Result<T>,
    ) -> Result<T>;

    /// Fetch first row, map with closure. None if empty.
    async fn query_optional_map<T>(
        &self, sql: &str, params: impl IntoParams,
        f: impl Fn(&libsql::Row) -> Result<T>,
    ) -> Result<Option<T>>;

    /// Fetch all rows, map with closure.
    async fn query_all_map<T>(
        &self, sql: &str, params: impl IntoParams,
        f: impl Fn(&libsql::Row) -> Result<T>,
    ) -> Result<Vec<T>>;

    /// Start a SelectBuilder for composable filter + pagination.
    fn select(&self, sql: &str) -> SelectBuilder<'_>;
}
```

Implemented for both `libsql::Connection` and `libsql::Transaction`.

### Usage

```rust
use modo::ldb::ConnExt;

// With FromRow
let user: User = conn.query_one("SELECT id, name, email FROM users WHERE id = ?1", params![id]).await?;
let users: Vec<User> = conn.query_all("SELECT id, name, email FROM users", ()).await?;
let maybe: Option<User> = conn.query_optional("SELECT ... WHERE id = ?1", params![id]).await?;

// With closure (no FromRow needed)
let user = conn.query_one_map("SELECT id, name FROM users WHERE id = ?1", params![id], |row| {
    Ok(User { id: row.get(0)?, name: row.get(1)?, email: String::new() })
}).await?;

// Raw libsql — always available on the same connection
let mut rows = conn.query("SELECT ...", ()).await?;
let tx = conn.transaction().await?;
```

## FromRow Trait

```rust
pub trait FromRow: Sized {
    fn from_row(row: &libsql::Row) -> Result<Self>;
}
```

Two approaches for implementing, user chooses per struct:

### Positional (fast, manual)

```rust
impl FromRow for User {
    fn from_row(row: &libsql::Row) -> Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            name: row.get(1)?,
            email: row.get(2)?,
        })
    }
}
```

### Name-based (resilient, uses ColumnMap)

```rust
impl FromRow for User {
    fn from_row(row: &libsql::Row) -> Result<Self> {
        let c = ColumnMap::from_row(row);
        Ok(Self {
            id: c.get(row, "id")?,
            name: c.get(row, "name")?,
            email: c.get(row, "email")?,
        })
    }
}
```

### ColumnMap

```rust
/// Builds a name → column index lookup from a Row's metadata.
/// Constructed once per query result, reused for all rows.
pub struct ColumnMap { /* HashMap<&str, i32> */ }

impl ColumnMap {
    /// Build lookup from row column metadata.
    pub fn from_row(row: &libsql::Row) -> Self;

    /// Get typed value by column name. Returns Error if column not found.
    pub fn get<T: FromValue>(&self, row: &libsql::Row, name: &str) -> Result<T>;
}
```

For `query_all` and friends, the `ColumnMap` is built once from the first row and reused for all subsequent rows.

## SelectBuilder

Composable query builder that combines filters, sorting, and pagination.

```rust
pub struct SelectBuilder<'a> {
    conn: &'a libsql::Connection,
    base_sql: String,
    filter: Option<ValidatedFilter>,
    order_by: Option<String>,
}
```

### API

```rust
impl<'a> SelectBuilder<'a> {
    /// Apply a validated filter (WHERE clauses).
    pub fn filter(self, filter: ValidatedFilter) -> Self;

    /// Set ORDER BY clause. Raw SQL — not user input.
    pub fn order_by(self, order: &str) -> Self;

    /// Execute with offset pagination. Returns Page<T>.
    pub async fn page<T: FromRow>(self, req: PageRequest) -> Result<Page<T>>;

    /// Execute with cursor pagination. Returns CursorPage<T>.
    pub async fn cursor<T: FromRow>(self, req: CursorRequest) -> Result<CursorPage<T>>;

    /// Execute without pagination. Returns Vec<T>.
    pub async fn fetch_all<T: FromRow>(self) -> Result<Vec<T>>;

    /// Execute without pagination. Returns first row.
    pub async fn fetch_one<T: FromRow>(self) -> Result<T>;

    /// Execute without pagination. Returns Option<T>.
    pub async fn fetch_optional<T: FromRow>(self) -> Result<Option<T>>;
}
```

### Usage

```rust
// Full composition: filter + sort + paginate
let page: Page<User> = conn
    .select("SELECT id, name, email, status FROM users")
    .filter(filter)
    .order_by("created_at DESC")
    .page(page_request)
    .await?;

// Filter + cursor pagination
let page: CursorPage<User> = conn
    .select("SELECT id, name, email FROM users")
    .filter(filter)
    .cursor(cursor_request)
    .await?;

// Filter only, no pagination
let users: Vec<User> = conn
    .select("SELECT id, name, email FROM users")
    .filter(filter)
    .fetch_all()
    .await?;

// Pagination only, no filter
let page: Page<User> = conn
    .select("SELECT id, name, email FROM users")
    .page(page_request)
    .await?;
```

### SQL generation

For offset pagination, SelectBuilder generates two queries:
1. `SELECT COUNT(*) FROM ({base_sql} WHERE {filters})` — total count
2. `{base_sql} WHERE {filters} ORDER BY {order} LIMIT {per_page} OFFSET {offset}` — data

For cursor pagination:
1. `{base_sql} WHERE {filters} AND id > ?cursor ORDER BY id ASC LIMIT {per_page + 1}` — fetch one extra to determine `has_more`

All parameter values accumulated as `Vec<libsql::Value>` and passed as a single params list.

## Filter + FilterSchema

### FilterSchema

Defines which columns are filterable and sortable for an endpoint.

```rust
pub struct FilterSchema {
    fields: Vec<(String, FieldType)>,
    sort_fields: Vec<String>,
}

pub enum FieldType {
    Text,
    Int,
    Float,
    Date,
    Bool,
}

impl FilterSchema {
    pub fn new() -> Self;
    pub fn field(self, name: &str, typ: FieldType) -> Self;
    pub fn sort_fields(self, fields: &[&str]) -> Self;
}
```

### Filter

Extracted from HTTP query string via `FromRequestParts`.

```rust
/// Raw parsed filter from query string. Must be validated before use.
pub struct Filter { /* parsed key-operator-value triples */ }

/// Validated filter — safe to use in SQL.
pub struct ValidatedFilter { /* WHERE clauses + params */ }

impl Filter {
    /// Validate against schema. Returns validated filter or 400 error.
    pub fn validate(self, schema: &FilterSchema) -> Result<ValidatedFilter>;
}
```

`Filter` implements `FromRequestParts` — extracted automatically from the query string. Unknown columns are silently ignored during validation. Type mismatches (e.g., `?age=abc` when `age` is `FieldType::Int`) return 400 error.

### Query string DSL

| Query param | SQL generated | HTML element |
|-------------|---------------|--------------|
| `?status=active` | `status = ?` | text, radio, single select |
| `?status=active&status=pending` | `status IN (?, ?)` | checkboxes, multi-select |
| `?age.gt=18` | `age > ?` | text input |
| `?age.gte=18` | `age >= ?` | text input |
| `?age.lt=65` | `age < ?` | text input |
| `?age.lte=65` | `age <= ?` | text input |
| `?name.like=john%` | `name LIKE ?` | text input |
| `?status.ne=banned` | `status != ?` | text input |
| `?deleted_at.null=true` | `deleted_at IS NULL` | checkbox |
| `?deleted_at.null=false` | `deleted_at IS NOT NULL` | checkbox |
| `?sort=-created_at` | `ORDER BY created_at DESC` | select |
| `?sort=name` | `ORDER BY name ASC` | select |

Rules:
- All values parameterized — never interpolated into SQL
- Column names validated against schema whitelist — never interpolated raw
- Unknown columns silently ignored
- Multiple values for same key → automatic `IN` clause
- Sort prefix `-` means DESC, no prefix means ASC
- Sort field must be in `sort_fields` whitelist or ignored
- Operators: `.gt`, `.gte`, `.lt`, `.lte`, `.ne`, `.like`, `.null`

### Handler example

```rust
static USER_FILTERS: LazyLock<FilterSchema> = LazyLock::new(|| FilterSchema::new()
    .field("status", FieldType::Text)
    .field("role", FieldType::Text)
    .field("created_at", FieldType::Date)
    .field("age", FieldType::Int)
    .sort_fields(&["created_at", "name", "age"]));

async fn list_users(
    Service(db): Service<ldb::Database>,
    filter: ldb::Filter,
    page: ldb::PageRequest,
) -> Result<Json<ldb::Page<User>>> {
    let filter = filter.validate(&USER_FILTERS)?;

    let page = db.conn()
        .select("SELECT id, name, email, status, role, created_at, age FROM users")
        .filter(filter)
        .page(page)
        .await?;

    Ok(Json(page))
}
```

## Pagination Types

### Offset-based

```rust
pub struct Page<T: Serialize> {
    pub items: Vec<T>,
    pub total: i64,
    pub page: i64,
    pub per_page: i64,
    pub total_pages: i64,
    pub has_next: bool,
    pub has_prev: bool,
}

pub struct PageRequest {
    pub page: i64,
    pub per_page: i64,
}
```

`PageRequest` implements `FromRequestParts`. Extracts from `?page=N&per_page=N`. Clamps values using `PaginationConfig` from request extensions.

### Cursor-based

```rust
pub struct CursorPage<T: Serialize> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
    pub per_page: i64,
}

pub struct CursorRequest {
    pub after: Option<String>,
    pub per_page: i64,
}
```

`CursorRequest` implements `FromRequestParts`. Extracts from `?after=<cursor>&per_page=N`.

### Config

```rust
pub struct PaginationConfig {
    pub default_per_page: i64,  // default: 20
    pub max_per_page: i64,      // default: 100
}
```

Added to app state. Used by `PageRequest` and `CursorRequest` extractors for clamping.

## Migration Runner

```rust
/// Run SQL migrations from directory against connection.
pub async fn migrate(conn: &libsql::Connection, dir: &str) -> Result<()>;
```

Behavior:
1. Create `_migrations` table if not exists: `(name TEXT PRIMARY KEY, checksum TEXT NOT NULL, applied_at TEXT NOT NULL DEFAULT (datetime('now')))`
2. Read `*.sql` files from `dir`, sorted by filename
3. For each file:
   - Compute MD5 checksum of file content
   - Check if already applied (exists in `_migrations`)
   - If applied: verify checksum matches. Error if modified.
   - If not applied: execute SQL, insert into `_migrations`
4. Forward-only, no rollback

Called automatically by `connect()` when `config.migrations` is set. Can also be called manually.

## Error Conversion

```rust
impl From<libsql::Error> for modo::Error {
    fn from(err: libsql::Error) -> Self {
        match &err {
            libsql::Error::SqliteFailure(code, _) => match code {
                SQLITE_CONSTRAINT_UNIQUE => Error::conflict("record already exists"),
                SQLITE_CONSTRAINT_FOREIGNKEY => Error::bad_request("foreign key violation"),
                _ => Error::internal("database error").chain(err),
            },
            _ => Error::internal("database error").chain(err),
        }
    }
}
```

Exact SQLite error codes will be mapped during implementation based on libsql's error variants.

## Graceful Shutdown

```rust
pub struct ManagedDatabase(Database);

impl Task for ManagedDatabase {
    async fn shutdown(self) -> Result<()> {
        // Connection dropped, libsql handles cleanup
        Ok(())
    }
}

pub fn managed(db: Database) -> ManagedDatabase {
    ManagedDatabase(db)
}
```

Usage: `modo::run!(server, ldb::managed(db)).await;`

## Public API (lib.rs re-exports)

```rust
// From ldb module
pub use ldb::{
    // Core
    Config, Database, connect, managed, ManagedDatabase, migrate,
    // Traits
    ConnExt, FromRow, ColumnMap,
    // Filter
    Filter, ValidatedFilter, FilterSchema, FieldType,
    // Pagination
    Page, CursorPage, PageRequest, CursorRequest, PaginationConfig,
    // Builder
    SelectBuilder,
    // Config enums
    JournalMode, SynchronousMode, TempStore,
};

// Re-export libsql for direct access
pub use libsql;
```

## Out of Scope

- Migration of existing modo modules (session, job, etc.) to ldb
- Removal of existing db and page modules
- Multiple connection patterns (pool, read/write split)
- Vector search helpers (use raw libsql for now)
- Derive macro for FromRow
