# Database (`modo::db`)

Lightweight libsql (SQLite) database layer. Single connection per handle, no ORM. Optional `DatabasePool` for multi-database sharding. One module: `src/db/`.

Feature flag: `db` (default). Dependencies: `libsql`, `urlencoding`.

Re-exports `libsql` crate for direct access to `libsql::params!`, `libsql::Value`, `libsql::Connection`, `libsql::Transaction`.

## Database Handle

`Database` — clone-able, `Arc`-wrapped single-connection handle. Created by `connect()`.

```rust
#[derive(Clone)]
pub struct Database { /* inner: Arc<Inner> */ }

impl Database {
    pub fn conn(&self) -> &libsql::Connection;
}
```

`Inner` is private. `new()` is `pub(crate)`.

## Configuration (`Config`)

Derives `Debug`, `Clone`, `Deserialize`. All fields have serde defaults. `impl Default for Config` mirrors those defaults.

| Field          | Type              | Default              |
| -------------- | ----------------- | -------------------- |
| `path`         | `String`          | `"data/app.db"`      |
| `migrations`   | `Option<String>`  | `None`               |
| `busy_timeout` | `u64`             | `5000` ms            |
| `cache_size`   | `i64`             | `16384` (~16 MB)     |
| `mmap_size`    | `u64`             | `268_435_456` (256 MB) |
| `journal_mode` | `JournalMode`     | `Wal`                |
| `synchronous`  | `SynchronousMode` | `Normal`             |
| `foreign_keys` | `bool`            | `true`               |
| `temp_store`   | `TempStore`       | `Memory`             |
| `pool`         | `Option<PoolConfig>` | `None`            |

### PoolConfig

Derives `Debug`, `Clone`, `Deserialize`. Nested inside `Config` to enable `DatabasePool`.

| Field          | Type     | Default          |
| -------------- | -------- | ---------------- |
| `base_path`    | `String` | `"data/shards"`  |
| `lock_shards`  | `usize`  | `16`             |

### Enum types

**`JournalMode`** — derives `Debug, Clone, Copy, Deserialize, Default`. Variants: `Wal` (default), `Delete`, `Truncate`, `Memory`, `Off`. Serde: `rename_all = "lowercase"`.

```rust
impl JournalMode {
    pub fn as_str(self) -> &'static str;
}
```

**`SynchronousMode`** — derives `Debug, Clone, Copy, Deserialize, Default`. Variants: `Off`, `Normal` (default), `Full`, `Extra`. Serde: `rename_all = "lowercase"`.

```rust
impl SynchronousMode {
    pub fn as_str(self) -> &'static str;
}
```

**`TempStore`** — derives `Debug, Clone, Copy, Deserialize, Default`. Variants: `Default`, `File`, `Memory` (default). Serde: `rename_all = "lowercase"`.

```rust
impl TempStore {
    pub fn as_str(self) -> &'static str;
}
```

## Connection Functions

### `connect(config: &Config) -> Result<Database>`

Opens a local libsql database, applies PRAGMAs from `Config`, optionally runs migrations (when `Config::migrations` is set). Creates parent directories for the database path if they don't exist.

```rust
pub async fn connect(config: &Config) -> Result<Database>;
```

### `migrate(conn: &libsql::Connection, dir: &str) -> Result<()>`

Runs `*.sql` migration files from a directory, sorted by filename. Tracks applied migrations in a `_migrations` table with FNV-1a checksum verification. Each migration runs in a transaction. Skips already-applied files. Errors on checksum mismatch.

```rust
pub async fn migrate(conn: &libsql::Connection, dir: &str) -> Result<()>;
```

## ConnExt Trait

Low-level query trait implemented for `libsql::Connection` and `libsql::Transaction`.

```rust
pub trait ConnExt: Sync {
    fn query_raw(
        &self,
        sql: &str,
        params: impl IntoParams + Send,
    ) -> impl Future<Output = std::result::Result<libsql::Rows, libsql::Error>> + Send;

    fn execute_raw(
        &self,
        sql: &str,
        params: impl IntoParams + Send,
    ) -> impl Future<Output = std::result::Result<u64, libsql::Error>> + Send;

    fn select<'a>(&'a self, sql: &str) -> SelectBuilder<'a, Self>
    where
        Self: Sized;
}
```

Implemented for: `libsql::Connection`, `libsql::Transaction`.

## ConnQueryExt Trait

High-level query helpers. Blanket-implemented for all `ConnExt` types (`impl<T: ConnExt> ConnQueryExt for T {}`).

```rust
pub trait ConnQueryExt: ConnExt {
    fn query_one<T: FromRow + Send>(
        &self,
        sql: &str,
        params: impl IntoParams + Send,
    ) -> impl Future<Output = Result<T>> + Send;

    fn query_optional<T: FromRow + Send>(
        &self,
        sql: &str,
        params: impl IntoParams + Send,
    ) -> impl Future<Output = Result<Option<T>>> + Send;

    fn query_all<T: FromRow + Send>(
        &self,
        sql: &str,
        params: impl IntoParams + Send,
    ) -> impl Future<Output = Result<Vec<T>>> + Send;

    fn query_one_map<T: Send, F>(
        &self,
        sql: &str,
        params: impl IntoParams + Send,
        f: F,
    ) -> impl Future<Output = Result<T>> + Send
    where
        F: Fn(&libsql::Row) -> Result<T> + Send;

    fn query_optional_map<T: Send, F>(
        &self,
        sql: &str,
        params: impl IntoParams + Send,
        f: F,
    ) -> impl Future<Output = Result<Option<T>>> + Send
    where
        F: Fn(&libsql::Row) -> Result<T> + Send;

    fn query_all_map<T: Send, F>(
        &self,
        sql: &str,
        params: impl IntoParams + Send,
        f: F,
    ) -> impl Future<Output = Result<Vec<T>>> + Send
    where
        F: Fn(&libsql::Row) -> Result<T> + Send;
}
```

## Row Mapping

### `FromRow` trait

```rust
pub trait FromRow: Sized {
    fn from_row(row: &libsql::Row) -> Result<Self>;
}
```

### `ColumnMap`

Column name to index lookup built from a single row's column metadata.

```rust
pub struct ColumnMap { /* map: HashMap<String, i32> */ }

impl ColumnMap {
    pub fn from_row(row: &libsql::Row) -> Self;
    pub fn index(&self, name: &str) -> Result<i32>;
    pub fn get<T: FromValue>(&self, row: &libsql::Row, name: &str) -> Result<T>;
}
```

### `FromValue` trait

Converts `libsql::Value` into a concrete Rust type. Implemented for: `libsql::Value`, `String`, `i32`, `u32`, `i64`, `u64`, `f64`, `bool`, `Vec<u8>`, `Option<T>` (where `T: FromValue`).

```rust
pub trait FromValue: Sized {
    fn from_value(val: libsql::Value) -> Result<Self>;
}
```

## Filtering

### `FilterSchema`

Declares allowed filter fields and sort fields for an endpoint. Derives `Default`.

```rust
pub struct FilterSchema { /* fields, sort_fields */ }

impl FilterSchema {
    pub fn new() -> Self;
    pub fn field(mut self, name: &str, typ: FieldType) -> Self;
    pub fn sort_fields(mut self, fields: &[&str]) -> Self;
}
```

### `FieldType`

Derives `Debug, Clone, Copy`. Variants: `Text`, `Int`, `Float`, `Date`, `Bool`.

### `Filter`

Raw parsed filter from query string. Implements `FromRequestParts<S>` (axum extractor).

Supported query-string syntax:

| Pattern          | Meaning                           |
| ---------------- | --------------------------------- |
| `field=value`    | Equality, or `IN` if multi-value  |
| `field.ne=value` | Not equal                         |
| `field.gt=value` | Greater than                      |
| `field.gte=value`| Greater than or equal             |
| `field.lt=value` | Less than                         |
| `field.lte=value`| Less than or equal                |
| `field.like=value`| SQL `LIKE`                       |
| `field.null=true`| `IS NULL` / `IS NOT NULL`         |
| `sort=field`     | Sort ascending; `-field` for desc; repeat for multi-column |

```rust
pub struct Filter { /* conditions, sort */ }

impl Filter {
    pub fn from_query_params(params: &HashMap<String, Vec<String>>) -> Self;
    pub fn validate(self, schema: &FilterSchema) -> Result<ValidatedFilter>;
}
```

### `ValidatedFilter`

`#[non_exhaustive]`. Schema-validated filter safe for SQL generation.

```rust
pub struct ValidatedFilter {
    pub clauses: Vec<String>,
    pub params: Vec<libsql::Value>,
    pub sort_clause: Option<String>,
}

impl ValidatedFilter {
    pub fn is_empty(&self) -> bool;
}
```

## Pagination

### `PaginationConfig`

Derives `Debug, Clone`. Add to request extensions to override defaults.

```rust
pub struct PaginationConfig {
    pub default_per_page: i64,  // default: 20
    pub max_per_page: i64,      // default: 100
}
```

### `PageRequest`

Offset pagination extractor (`?page=N&per_page=N`). Derives `Debug, Clone, Deserialize`. Implements `FromRequestParts<S>`. Pages are 1-based.

```rust
pub struct PageRequest {
    pub page: i64,
    pub per_page: i64,
}

impl PageRequest {
    pub fn clamp(&mut self, config: &PaginationConfig);
    pub fn offset(&self) -> i64;
}
```

### `Page<T: Serialize>`

Offset-based page response. Derives `Debug, Serialize`.

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

impl<T: Serialize> Page<T> {
    pub fn new(items: Vec<T>, total: i64, page: i64, per_page: i64) -> Self;
}
```

### `CursorRequest`

Cursor pagination extractor (`?after=<cursor>&per_page=N`). Derives `Debug, Clone, Deserialize`. Implements `FromRequestParts<S>`.

```rust
pub struct CursorRequest {
    pub after: Option<String>,
    pub per_page: i64,
}

impl CursorRequest {
    pub fn clamp(&mut self, config: &PaginationConfig);
}
```

### `CursorPage<T: Serialize>`

Cursor-based page response. Derives `Debug, Serialize`.

```rust
pub struct CursorPage<T: Serialize> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
    pub per_page: i64,
}

impl<T: Serialize> CursorPage<T> {
    pub fn new(items: Vec<T>, next_cursor: Option<String>, per_page: i64) -> Self;
}
```

## SelectBuilder

Composable query builder combining filters, sorting, and pagination. Created via `ConnExt::select()`.

```rust
pub struct SelectBuilder<'a, C: ConnExt> { /* conn, base_sql, filter, order_by, cursor_column, cursor_desc */ }

impl<'a, C: ConnExt> SelectBuilder<'a, C> {
    pub fn filter(mut self, filter: ValidatedFilter) -> Self;
    pub fn order_by(mut self, order: &str) -> Self;
    pub fn cursor_column(mut self, col: &str) -> Self;
    pub fn oldest_first(mut self) -> Self;
    pub async fn page<T: FromRow + Serialize>(self, req: PageRequest) -> Result<Page<T>>;
    pub async fn cursor<T: FromRow + Serialize>(self, req: CursorRequest) -> Result<CursorPage<T>>;
    pub async fn fetch_all<T: FromRow>(self) -> Result<Vec<T>>;
    pub async fn fetch_one<T: FromRow>(self) -> Result<T>;
    pub async fn fetch_optional<T: FromRow>(self) -> Result<Option<T>>;
}
```

Default cursor column: `"id"`. Default cursor order: descending (newest-first). `oldest_first()` switches to ascending.

## Managed Database (Graceful Shutdown)

`ManagedDatabase` implements `Task`. Wraps a `Database` for use with `run!` macro. On shutdown, drops the database handle.

```rust
pub struct ManagedDatabase(Database);

pub fn managed(db: Database) -> ManagedDatabase;
```

## DatabasePool (Multi-Database Sharding)

`DatabasePool` — manages a default `Database` plus lazily-opened shard databases. Wraps `Arc<Inner>`, cheap to clone. Created by `DatabasePool::new()`.

```rust
#[derive(Clone)]
pub struct DatabasePool { /* inner: Arc<Inner> */ }

impl DatabasePool {
    pub async fn new(config: &Config) -> Result<Self>;
    pub async fn conn(&self, shard: Option<&str>) -> Result<Database>;
}
```

### `new(config: &Config) -> Result<Self>`

Opens the default database immediately. Shard databases are opened lazily on first `conn()` call. Requires `config.pool` to be `Some`. Rejects `lock_shards == 0`.

### `conn(&self, shard: Option<&str>) -> Result<Database>`

- `None` — returns the default database (instant, no lock).
- `Some("name")` — returns the cached shard database, opening it on first access at `{base_path}/{name}.db`.

Rejects invalid shard names (empty, starts with `.`, contains `/`, `\`, or `\0`) as 400 errors — path-traversal prevention. Concurrent first-access to the same shard may open duplicate connections; last writer wins (benign — `connect` is idempotent).

### `ManagedDatabasePool`

Implements `Task`. Wraps a `DatabasePool` for use with `run!` macro. On shutdown, drops the pool handle.

```rust
pub struct ManagedDatabasePool(DatabasePool);

pub fn managed_pool(pool: DatabasePool) -> ManagedDatabasePool;
```

## libsql Error Conversion

`libsql::Error` converts into `modo::Error` via `From`:

| libsql error                                        | HTTP status | message                          |
| --------------------------------------------------- | ----------- | -------------------------------- |
| `SqliteFailure(2067)` / `SqliteFailure(1555)` (unique/PK) | 409   | "record already exists"          |
| `SqliteFailure(787)` (FK violation)                 | 400         | "foreign key violation"          |
| `SqliteFailure(other)`                              | 500         | "database error: {msg}"          |
| `QueryReturnedNoRows`                               | 404         | "record not found"               |
| `NullValue`                                         | 400         | "unexpected null value"          |
| `ConnectionFailed(msg)`                             | 500         | "database connection failed: {msg}" |
| `InvalidColumnIndex`                                | 500         | "invalid column index"           |
| `InvalidColumnType`                                 | 500         | "invalid column type"            |
| all others                                          | 500         | "database error"                 |

## Usage Examples

```rust
use modo::db;

// Connect with defaults (data/app.db, WAL mode, FK on)
let db = db::connect(&db::Config::default()).await?;

// Use ConnQueryExt for typed queries
use db::ConnQueryExt;
let user: User = db.conn().query_one(
    "SELECT id, name FROM users WHERE id = ?1",
    libsql::params!["user_abc"],
).await?;

// Optional query
let maybe: Option<User> = db.conn().query_optional(
    "SELECT id, name FROM users WHERE email = ?1",
    libsql::params![email],
).await?;

// All rows
let users: Vec<User> = db.conn().query_all(
    "SELECT id, name FROM users WHERE active = ?1",
    libsql::params![1],
).await?;

// Closure-based mapping
let count: i64 = db.conn().query_one_map(
    "SELECT COUNT(*) FROM users",
    (),
    |row| Ok(row.get::<i64>(0).map_err(modo::Error::from)?),
).await?;

// Raw execute
use db::ConnExt;
let affected = db.conn().execute_raw(
    "DELETE FROM sessions WHERE expires_at < datetime('now')",
    (),
).await.map_err(modo::Error::from)?;

// SelectBuilder with filters and pagination
let page = db.conn()
    .select("SELECT id, name FROM users")
    .filter(validated_filter)
    .order_by("\"created_at\" DESC")
    .page::<User>(page_request)
    .await?;

// Cursor pagination
let cursor_page = db.conn()
    .select("SELECT id, name FROM users")
    .filter(validated_filter)
    .cursor_column("id")
    .cursor::<User>(cursor_request)
    .await?;

// Transactions
let tx = db.conn().transaction().await.map_err(modo::Error::from)?;
tx.execute_raw("INSERT INTO users (id, name) VALUES (?1, ?2)", libsql::params!["id1", "Alice"]).await?;
tx.execute_raw("INSERT INTO profiles (user_id) VALUES (?1)", libsql::params!["id1"]).await?;
tx.commit().await.map_err(modo::Error::from)?;
```

## Implementing FromRow

```rust
use modo::db::{FromRow, ColumnMap};
use modo::Result;

struct User {
    id: String,
    name: String,
    age: Option<i32>,
}

impl FromRow for User {
    fn from_row(row: &libsql::Row) -> Result<Self> {
        let cols = ColumnMap::from_row(row);
        Ok(Self {
            id: cols.get(row, "id")?,
            name: cols.get(row, "name")?,
            age: cols.get(row, "age")?,
        })
    }
}
```

## Maintenance (Health Check & VACUUM)

### `DbHealth`

Page-level health metrics from PRAGMA introspection. Derives `Debug, Clone`. Does **not** derive `Serialize` — internal metrics that must not be exposed on unauthenticated endpoints.

```rust
pub struct DbHealth {
    pub page_count: u64,
    pub freelist_count: u64,
    pub page_size: u64,
    pub free_percent: f64,       // 0.0–100.0
    pub total_size_bytes: u64,   // page_count * page_size
    pub wasted_bytes: u64,       // freelist_count * page_size
}

impl DbHealth {
    pub async fn collect(conn: &libsql::Connection) -> Result<Self>;
    pub fn needs_vacuum(&self, threshold_percent: f64) -> bool;
}
```

`collect` runs `PRAGMA page_count`, `PRAGMA freelist_count`, `PRAGMA page_size` and computes derived fields. `needs_vacuum` returns `true` if `free_percent >= threshold_percent`.

### `VacuumOptions`

Derives `Debug, Clone`. Implements `Default` (threshold: `20.0`, dry_run: `false`).

```rust
pub struct VacuumOptions {
    pub threshold_percent: f64,  // default: 20.0
    pub dry_run: bool,           // default: false
}
```

### `VacuumResult`

Derives `Debug, Clone`.

```rust
pub struct VacuumResult {
    pub health_before: DbHealth,
    pub health_after: Option<DbHealth>,  // None if skipped or dry_run
    pub vacuumed: bool,
    pub duration: std::time::Duration,
}
```

### `async run_vacuum(conn: &libsql::Connection, opts: VacuumOptions) -> Result<VacuumResult>`

Run VACUUM with safety checks: collects health, checks threshold, executes `VACUUM` if needed, collects health again. Logs before/after metrics at `debug` level. Skips if `dry_run` or below threshold (returns `vacuumed: false`, `health_after: None`).

### `async vacuum_if_needed(conn: &libsql::Connection, threshold_percent: f64) -> Result<VacuumResult>`

Shorthand for `run_vacuum` with the given threshold and default options.

### `vacuum_handler(threshold_percent: f64) -> VacuumHandler`

Returns a cron handler implementing `CronHandler<(Service<Database>,)>`. Extracts `Service<Database>` from the cron context, calls `run_vacuum`, logs results at `info` level. `VacuumHandler` derives `Clone`, is public (as return type) but has private fields — construct via `vacuum_handler()` only.

```rust
use modo::cron::Scheduler;
use modo::db;

let scheduler = Scheduler::builder(&registry)
    .job("0 3 * * 0", db::vacuum_handler(20.0))?
    .start()
    .await;
```

## Gotchas

- **Single connection per handle**: `Database` wraps one `libsql::Connection` in `Arc`. All clones share the same connection. `DatabasePool` manages multiple `Database` handles (one per shard).

- **Pool memory at scale**: Each shard connection uses its own SQLite page cache (default 16 MB `cache_size`) and mmap region (default 256 MB virtual). At 100 shards: ~1.6 GB page cache. Consider lowering `cache_size` for large shard counts.

- **999 bind params limit**: SQLite limits a single statement to 999 bound parameters. Batch operations must chunk accordingly.

- **No `ON CONFLICT` with partial unique indexes**: Use plain `INSERT` and catch the unique violation error from the `From<libsql::Error>` conversion (409 Conflict).

- **Parent directories auto-created**: `connect()` creates missing parent directories for the database path.

- **PRAGMAs via query, not execute**: libsql PRAGMAs return rows, so `connect()` uses `conn.query()` (not `conn.execute()`) for PRAGMA statements.

- **`ConnExt` uses RPITIT**: `query_raw` and `execute_raw` return `impl Future` (not `Pin<Box<dyn Future>>`), so `ConnExt` is not object-safe.

- **Transactions**: Use `db.conn().transaction().await` to get a `libsql::Transaction`. `ConnExt` and `ConnQueryExt` are implemented for `Transaction`, so the same query helpers work inside transactions.

- **Migrations use FNV-1a**: Checksums are FNV-1a (not SHA/CRC), stored as 16-char hex. Deterministic and stable across Rust versions.
