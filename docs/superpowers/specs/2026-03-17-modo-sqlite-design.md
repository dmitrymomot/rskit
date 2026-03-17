# modo-sqlite: Pure sqlx SQLite Crate

**Date:** 2026-03-17
**Status:** Approved
**Scope:** New crate — connection pool management, read/write split, embedded SQL migrations

## Motivation

`modo-db` wraps SeaORM for ORM-style CRUD with auto-sync schema management. `modo-sqlite` is the alternative for developers who prefer writing raw SQL with full control — compile-time checked queries, repository pattern, no ORM abstraction layer.

Key features that justify the crate:

- **Read/write connection split** — 1 writer (serializes writes, no SQLITE_BUSY) + N readers (concurrent in WAL mode)
- **`embed_migrations!()`** — SQL file migrations with inventory-based auto-discovery across crates
- **Optimized per-pool PRAGMA config** — different defaults for simple vs high-load usage

Future `modo-pg` crate will follow the same pattern for Postgres. No dialect abstraction — each DB gets a purpose-built crate.

## Crate Structure

```
modo-sqlite/
  src/
    lib.rs            — mod declarations, pub use re-exports (no logic)
    config.rs         — DatabaseConfig, SqliteConfig, PRAGMA enums
    connect.rs        — connect(), connect_rw(), PRAGMA application via after_connect
    pool.rs           — Pool, ReadPool, WritePool, AsPool trait
    extractor.rs      — Db, DbReader, DbWriter
    migration.rs      — MigrationRegistration, run_migrations(), run_migrations_group(), run_migrations_except()
    id.rs             — generate_ulid(), generate_short_id()
    error.rs          — modo_sqlite::Error enum, From<sqlx::Error>, From<Error> for modo::Error
  Cargo.toml

modo-sqlite-macros/
  src/
    lib.rs            — embed_migrations!() proc macro
  Cargo.toml
```

### Dependencies

```
modo-sqlite       → sqlx (sqlite feature), inventory, modo, chrono, rand, thiserror, modo-sqlite-macros
modo-sqlite-macros → proc-macro2, quote, syn (compile-time only, reads filesystem via std::fs)
```

No SeaORM dependency anywhere.

## Configuration

Flat struct — no nesting. Pool settings and PRAGMAs at the same level:

```rust
#[derive(Debug, Clone, Copy, Deserialize, Default)]
#[serde(rename_all = "UPPERCASE")]
pub enum JournalMode { #[default] Wal, Delete, Truncate, Persist, Off }

#[derive(Debug, Clone, Copy, Deserialize, Default)]
#[serde(rename_all = "UPPERCASE")]
pub enum SynchronousMode { Full, #[default] Normal, Off }

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum TempStore { Default, File, Memory }

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct SqliteConfig {
    // Connection
    pub path: String,                        // "data/app.db" — crate builds sqlite:// URL internally
    pub max_connections: Option<u32>,        // None → connect()=10, connect_rw()=20
    pub min_connections: u32,                // 1
    pub writer_max_connections: u32,         // 1 (only used by connect_rw)
    pub acquire_timeout_secs: u64,           // 30
    pub idle_timeout_secs: u64,              // 600
    pub max_lifetime_secs: u64,              // 1800
    // PRAGMAs
    pub journal_mode: JournalMode,           // WAL
    pub busy_timeout: Option<u32>,           // None → connect()=5000, rw_writer=2000, rw_reader=1000
    pub synchronous: SynchronousMode,        // NORMAL
    pub foreign_keys: bool,                  // true
    pub cache_size: Option<i32>,             // None → connect()=-2000, connect_rw()=-16000
    pub mmap_size: Option<i64>,              // None → connect()=None, connect_rw()=268435456
    pub temp_store: Option<TempStore>,       // None
    pub wal_autocheckpoint: Option<u32>,     // None
}
```

The app config uses `sqlite` as the key, leaving room for `postgres` via future modo-pg:

```rust
// App's config.rs
pub struct Config {
    pub core: AppConfig,
    pub sqlite: modo_sqlite::SqliteConfig,
    // Future: pub postgres: modo_pg::PostgresConfig,
}
```

### Path vs URL

Users provide a plain file path (e.g. `data/app.db`). The crate internally:

- Creates parent directories if they don't exist
- Builds the sqlx URL: `sqlite://data/app.db?mode=rwc`
- Special case: `:memory:` → in-memory SQLite (only valid with `connect()`, see Gotchas)

### Smart Defaults per Connection Mode

`Option` fields resolve differently depending on which connect function is called. If a user sets a value explicitly, both functions respect it.

Resolution logic:

- If `max_connections` is `Some(n)`: `connect()` uses `n`, `connect_rw()` readers get `n`, writer gets `writer_max_connections`
- If `max_connections` is `None`: `connect()` uses 10, `connect_rw()` readers get 20, writer gets `writer_max_connections`

| Setting           | `connect()` (simple) | `connect_rw()` readers | `connect_rw()` writer                |
| ----------------- | -------------------- | ---------------------- | ------------------------------------ |
| `max_connections` | 10                   | 20                     | `writer_max_connections` (default 1) |
| `busy_timeout`    | 5000ms               | 1000ms                 | 2000ms                               |
| `cache_size`      | -2000 (2MB)          | -16000 (16MB)          | -16000 (16MB)                        |
| `mmap_size`       | None                 | 268435456 (256MB)      | 268435456 (256MB)                    |

YAML example:

```yaml
sqlite:
    path: "data/app.db"
    max_connections: 20
    busy_timeout: 5000
    cache_size: -64000
```

## Pool Types

Three newtype wrappers around `sqlx::SqlitePool`:

```rust
/// Single-pool mode. Used with connect().
pub struct Pool(sqlx::SqlitePool);

/// Reader pool for connect_rw(). Many concurrent connections, read-only queries.
pub struct ReadPool(sqlx::SqlitePool);

/// Writer pool for connect_rw(). Single connection, serializes all writes.
pub struct WritePool(sqlx::SqlitePool);
```

All three implement:

- `Clone` — cheap, inner pool is Arc'd by sqlx
- `pool(&self) -> &sqlx::SqlitePool` — access for sqlx queries
- `modo::GracefulShutdown` — calls `pool.close().await`

`AsPool` trait for migration runner — implemented only by `Pool` and `WritePool` (not `ReadPool`, since migrations execute DDL/writes):

```rust
pub trait AsPool {
    fn pool(&self) -> &sqlx::SqlitePool;
}

impl AsPool for Pool { ... }
impl AsPool for WritePool { ... }
// ReadPool intentionally excluded — migrations must run through a writable pool
```

Intentionally distinct types — compiler enforces read/write separation.

### Pool Lifecycle (automatic via sqlx)

| Behavior                         | Managed by              | Configured via                                          |
| -------------------------------- | ----------------------- | ------------------------------------------------------- |
| Open new connections on demand   | sqlx pool               | `min_connections` (eager), lazy up to `max_connections` |
| Close idle connections           | sqlx pool               | `idle_timeout_secs` (default 600s)                      |
| Replace expired connections      | sqlx pool               | `max_lifetime_secs` (default 1800s)                     |
| Wait for available connection    | sqlx pool               | `acquire_timeout_secs` (default 30s)                    |
| Apply PRAGMAs to new connections | `after_connect` hook    | `SqliteConfig` fields                                   |
| Graceful shutdown                | `GracefulShutdown` impl | automatic on app shutdown                               |

The `after_connect` hook fires once per connection creation (not per query). With `max_lifetime_secs: 1800` and `max_connections: 20`, that's ~20 PRAGMA executions per 30 minutes — effectively zero overhead.

## Connection Functions

```rust
/// Single pool — for simple apps.
pub async fn connect(config: &SqliteConfig) -> Result<Pool, modo::Error>;

/// Read/write split — for high-load apps.
/// Returns (reader_pool, writer_pool) with separate PRAGMA configs.
/// Errors if path is `:memory:` (in-memory databases are per-connection, split would create independent DBs).
pub async fn connect_rw(config: &SqliteConfig) -> Result<(ReadPool, WritePool), modo::Error>;
```

Both functions:

1. Resolve `config.path` → create parent dirs → build `sqlite://` URL
2. Build `sqlx::sqlite::SqlitePoolOptions` with pool sizing
3. Set `after_connect` closure applying PRAGMAs (with per-pool values for rw mode)
4. Return wrapped pool(s)

`connect_rw()` additionally validates that the path is not `:memory:`.

## Extractors

```rust
/// Single-pool extractor. Use with connect().
pub struct Db(pub Pool);

/// Reader extractor. Use with connect_rw().
pub struct DbReader(pub ReadPool);

/// Writer extractor. Use with connect_rw().
pub struct DbWriter(pub WritePool);
```

Each implements `FromRequestParts<AppState>` — pulls its pool type from modo's `ServiceRegistry`. Same pattern as modo-db: one `HashMap` lookup + one `Arc` clone per request.

Note: inner pool is `pub` for ergonomic access. The pool types are part of the public API.

### App Wiring — Simple Mode

```rust
let db = modo_sqlite::connect(&config.sqlite).await?;
modo_sqlite::run_migrations(&db).await?;
app.config(config.core).managed_service(db).run().await
```

### App Wiring — Read/Write Split

```rust
let (reader, writer) = modo_sqlite::connect_rw(&config.sqlite).await?;
modo_sqlite::run_migrations(&writer).await?;
app.config(config.core)
    .managed_service(reader)
    .managed_service(writer)
    .run().await
```

### Handler Signatures

```rust
// Simple mode
#[modo::handler(GET, "/todos")]
async fn list_todos(db: Db) -> JsonResult<Vec<TodoResponse>> { ... }

// Read/write split — type makes access pattern explicit
#[modo::handler(GET, "/todos")]
async fn list_todos(db: DbReader) -> JsonResult<Vec<TodoResponse>> { ... }

#[modo::handler(POST, "/todos")]
async fn create_todo(db: DbWriter, input: JsonReq<CreateTodo>) -> JsonResult<TodoResponse> { ... }
```

Rule: `DbWriter` for any SQL that modifies data (INSERT, UPDATE, DELETE — even with RETURNING). `DbReader` for pure SELECTs only.

Note: `id: String` in handler params works via modo's `#[handler]` macro which generates path extraction automatically. No explicit `PathReq` needed for params declared in the route pattern.

## Error Handling

### modo_sqlite::Error

Database-domain errors. No HTTP knowledge:

```rust
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("record not found")]
    NotFound,

    #[error("unique constraint violation: {0}")]
    UniqueViolation(String),

    #[error("foreign key violation: {0}")]
    ForeignKeyViolation(String),

    #[error("database pool timeout")]
    PoolTimeout,

    #[error("database error: {0}")]
    Query(#[from] sqlx::Error),
}
```

### From<sqlx::Error> for Error

Classifies raw sqlx errors into domain variants:

```rust
impl From<sqlx::Error> for Error {
    fn from(e: sqlx::Error) -> Self {
        match e {
            sqlx::Error::RowNotFound => Error::NotFound,
            sqlx::Error::Database(ref db_err) if db_err.is_unique_violation() => {
                Error::UniqueViolation(db_err.to_string())
            }
            sqlx::Error::Database(ref db_err) if db_err.is_foreign_key_violation() => {
                Error::ForeignKeyViolation(db_err.to_string())
            }
            sqlx::Error::PoolTimedOut => Error::PoolTimeout,
            other => Error::Query(other),
        }
    }
}
```

### From<Error> for modo::Error

Bridges to HTTP layer. Lets `?` auto-convert in handlers:

```rust
impl From<Error> for modo::Error {
    fn from(e: Error) -> Self {
        match e {
            Error::NotFound => HttpError::NotFound.into(),
            Error::UniqueViolation(msg) => HttpError::Conflict.with_message(msg),
            Error::ForeignKeyViolation(msg) => HttpError::BadRequest.with_message(msg),
            Error::PoolTimeout => modo::Error::internal("database pool timeout"),
            Error::Query(e) => modo::Error::internal(format!("database error: {e}")),
        }
    }
}
```

### Three-Layer Error Flow

- **Repository** → `Result<T, modo_sqlite::Error>` (database domain)
- **Handler** → `?` auto-converts to `modo::Error` via `From` (HTTP domain)
- **Response** → `modo::Error` renders JSON with status code

No manual error mapping needed anywhere.

## Migration System

### embed_migrations!() Proc Macro

Lives in `modo-sqlite-macros`. At compile time:

1. Reads `CARGO_MANIFEST_DIR/migrations/*.sql` (non-`.sql` files are ignored)
2. Parses filename: `{timestamp}_{description}.sql`
3. Embeds SQL content as `&'static str`
4. Emits `inventory::submit!` per file

**Filename parsing rules:**

- Timestamp must be exactly 14 digits (`YYYYMMDDHHmmss`) — compile error otherwise
- Non-numeric timestamp → compile error
- Missing `_` separator after timestamp → compile error
- Duplicate timestamps in the same directory → compile error at compile time

Generated code for `migrations/20260317120000_create_todos.sql`:

```rust
inventory::submit! {
    modo_sqlite::MigrationRegistration {
        version: 20260317120000,
        description: "create_todos",
        group: "default",
        sql: "CREATE TABLE IF NOT EXISTS todos (...)",
    }
}
```

Macro API:

```rust
// Auto-detects: path = "migrations/", group = "default"
modo_sqlite::embed_migrations!();

// Explicit overrides
modo_sqlite::embed_migrations!(path = "db/migrations", group = "jobs");
```

### MigrationRegistration

```rust
pub struct MigrationRegistration {
    pub version: u64,
    pub description: &'static str,
    pub group: &'static str,
    pub sql: &'static str,
}

inventory::collect!(MigrationRegistration);
```

### File Naming Convention

```
migrations/
  20260317120000_create_todos.sql
  20260317120100_add_priority_column.sql
  20260318090000_create_tags_table.sql
```

- Filename format: `{timestamp}_{description}.sql`
- Timestamp is `YYYYMMDDHHmmss` (14 digits) — used as the migration version
- Single `_` underscore separator between timestamp and description
- Description in snake_case
- `.sql` extension required
- Timestamps must be unique within a group

### Migration Table Schema

```sql
CREATE TABLE IF NOT EXISTS _modo_sqlite_migrations (
    version     BIGINT NOT NULL,
    grp         TEXT NOT NULL DEFAULT 'default',
    description TEXT NOT NULL,
    executed_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (grp, version)
)
```

The `group` column (`grp` to avoid SQL keyword conflict) is part of the composite primary key. This means:

- Version 1 in group "default" and version 1 in group "jobs" can coexist in the same database
- Each group's migrations are tracked independently
- `run_migrations_group()` and `run_migrations_except()` can query the table to see exactly what has been executed per group

modo-sqlite has no dependency on or compatibility requirement with modo-db. They are fully independent crates.

### Migration Runner

```rust
/// Run ALL pending migrations (every group).
pub async fn run_migrations(pool: &impl AsPool) -> Result<(), modo::Error>;

/// Run pending migrations for a specific group only.
pub async fn run_migrations_group(pool: &impl AsPool, group: &str) -> Result<(), modo::Error>;

/// Run pending migrations for all groups except the excluded ones.
pub async fn run_migrations_except(pool: &impl AsPool, exclude: &[&str]) -> Result<(), modo::Error>;
```

`AsPool` is only implemented by `Pool` and `WritePool` — the type system prevents passing a `ReadPool` to migration functions.

The runner:

1. Creates `_modo_sqlite_migrations` table if not exists
2. Collects `MigrationRegistration` from `inventory`
3. Filters by group (or excludes groups), sorts by version
4. Checks for duplicate versions within a group — error if found
5. Queries `_modo_sqlite_migrations` for already-executed versions (filtered by group)
6. Runs each pending migration's SQL in a transaction
7. Inserts record into `_modo_sqlite_migrations` with group

### Group Usage

```rust
// Simple app — single database, everything runs here
let db = modo_sqlite::connect(&config.database).await?;
modo_sqlite::run_migrations(&db).await?;

// App with separate jobs database — main gets everything except jobs
let db = modo_sqlite::connect(&config.database).await?;
modo_sqlite::run_migrations_except(&db, &["jobs"]).await?;

// Jobs database — only jobs group
let jobs_db = modo_sqlite::connect(&config.jobs_sqlite).await?;
modo_sqlite::run_migrations_group(&jobs_db, "jobs").await?;
```

## ID Generation

Same functions as modo-db (copied, not shared — no dependency between crates):

```rust
pub fn generate_ulid() -> String;      // 26-char Crockford Base32
pub fn generate_short_id() -> String;  // 13-char Base36, time-sortable
```

## Gotchas

- **`WritePool` single-connection deadlock:** With `max_connections: 1`, if a handler acquires the writer and then calls a function that internally tries to acquire the writer again (nested acquire), the pool will deadlock until `acquire_timeout_secs` expires. Never nest writer pool acquisitions — acquire once per handler and pass the connection through.
- **`:memory:` with `connect_rw()`:** In-memory SQLite databases are per-connection. Two pools would create independent databases. `connect_rw()` returns an error if path is `:memory:`. Use `connect()` for in-memory databases.
- **`writer_max_connections` > 1:** SQLite allows only one concurrent writer regardless of pool size. Setting `writer_max_connections` above 1 wastes connections and increases `SQLITE_BUSY` risk. The default of 1 is correct for virtually all cases.

## Complete Public API

```rust
// Config
modo_sqlite::SqliteConfig
modo_sqlite::JournalMode
modo_sqlite::SynchronousMode
modo_sqlite::TempStore

// Connect
modo_sqlite::connect(&sqlite_config) -> Result<Pool>
modo_sqlite::connect_rw(&sqlite_config) -> Result<(ReadPool, WritePool)>

// Pools
modo_sqlite::Pool
modo_sqlite::ReadPool
modo_sqlite::WritePool
modo_sqlite::AsPool  // trait (Pool + WritePool only)

// Extractors
modo_sqlite::Db
modo_sqlite::DbReader
modo_sqlite::DbWriter

// Migrations
modo_sqlite::embed_migrations!()
modo_sqlite::run_migrations(&pool)
modo_sqlite::run_migrations_group(&pool, group)
modo_sqlite::run_migrations_except(&pool, &[groups])
modo_sqlite::MigrationRegistration

// IDs
modo_sqlite::generate_ulid()
modo_sqlite::generate_short_id()

// Errors
modo_sqlite::Error
```

## Example: Todo API with Read/Write Split

### File Structure

```
todo-api/
  migrations/
    20260317120000_create_todos.sql
  src/
    main.rs
    config.rs
    entity.rs
    repository.rs
    handlers.rs
    types.rs
```

### migrations/20260317120000_create_todos.sql

```sql
CREATE TABLE IF NOT EXISTS todos (
    id          TEXT PRIMARY KEY,
    title       TEXT NOT NULL,
    completed   BOOLEAN NOT NULL DEFAULT FALSE,
    created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);
```

### entity.rs

```rust
#[derive(Debug, Clone, serde::Serialize, sqlx::FromRow)]
pub struct Todo {
    pub id: String,
    pub title: String,
    pub completed: bool,
    pub created_at: String,
    pub updated_at: String,
}
```

### repository.rs

```rust
use modo_sqlite::{DbReader, DbWriter, Error};
use crate::entity::Todo;

pub async fn find_all(db: &DbReader) -> Result<Vec<Todo>, Error> {
    Ok(sqlx::query_as::<_, Todo>("SELECT * FROM todos ORDER BY created_at DESC")
        .fetch_all(db.pool())
        .await?)
}

pub async fn find_by_id(id: &str, db: &DbReader) -> Result<Todo, Error> {
    sqlx::query_as::<_, Todo>("SELECT * FROM todos WHERE id = ?")
        .bind(id)
        .fetch_optional(db.pool())
        .await?
        .ok_or(Error::NotFound)
}

pub async fn insert(title: &str, db: &DbWriter) -> Result<Todo, Error> {
    let id = modo_sqlite::generate_ulid();
    Ok(sqlx::query_as::<_, Todo>(
        "INSERT INTO todos (id, title) VALUES (?, ?) RETURNING *"
    )
    .bind(&id)
    .bind(title)
    .fetch_one(db.pool())
    .await?)
}

pub async fn toggle(id: &str, db: &DbWriter) -> Result<Todo, Error> {
    sqlx::query_as::<_, Todo>(
        "UPDATE todos SET completed = NOT completed,
         updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
         WHERE id = ? RETURNING *"
    )
    .bind(id)
    .fetch_optional(db.pool())
    .await?
    .ok_or(Error::NotFound)
}

pub async fn delete(id: &str, db: &DbWriter) -> Result<(), Error> {
    let result = sqlx::query("DELETE FROM todos WHERE id = ?")
        .bind(id)
        .execute(db.pool())
        .await?;
    if result.rows_affected() == 0 {
        return Err(Error::NotFound);
    }
    Ok(())
}
```

### handlers.rs

```rust
use modo::extractor::JsonReq;
use modo::{Json, JsonResult};
use modo_sqlite::{DbReader, DbWriter};
use serde_json::{Value, json};
use crate::repository;
use crate::types::{CreateTodo, TodoResponse};

#[modo::handler(GET, "/todos")]
async fn list_todos(db: DbReader) -> JsonResult<Vec<TodoResponse>> {
    let todos = repository::find_all(&db).await?;
    Ok(Json(todos.into_iter().map(TodoResponse::from).collect()))
}

#[modo::handler(POST, "/todos")]
async fn create_todo(db: DbWriter, input: JsonReq<CreateTodo>) -> JsonResult<TodoResponse> {
    input.validate()?;
    let todo = repository::insert(&input.title, &db).await?;
    Ok(Json(TodoResponse::from(todo)))
}

#[modo::handler(GET, "/todos/{id}")]
async fn get_todo(db: DbReader, id: String) -> JsonResult<TodoResponse> {
    let todo = repository::find_by_id(&id, &db).await?;
    Ok(Json(TodoResponse::from(todo)))
}

#[modo::handler(PATCH, "/todos/{id}")]
async fn toggle_todo(db: DbWriter, id: String) -> JsonResult<TodoResponse> {
    let todo = repository::toggle(&id, &db).await?;
    Ok(Json(TodoResponse::from(todo)))
}

#[modo::handler(DELETE, "/todos/{id}")]
async fn delete_todo(db: DbWriter, id: String) -> JsonResult<Value> {
    repository::delete(&id, &db).await?;
    Ok(Json(json!({"deleted": id})))
}
```

### main.rs

```rust
mod config;
mod entity;
mod handlers;
mod repository;
mod types;

modo_sqlite::embed_migrations!();

#[modo::main]
async fn main(
    app: modo::app::AppBuilder,
    config: config::Config,
) -> Result<(), Box<dyn std::error::Error>> {
    let (reader, writer) = modo_sqlite::connect_rw(&config.sqlite).await?;
    modo_sqlite::run_migrations(&writer).await?;
    app.config(config.core)
        .managed_service(reader)
        .managed_service(writer)
        .run()
        .await
}
```

## Future: modo-pg

A separate `modo-pg` crate will follow the same patterns (pool wrappers, extractors, `embed_migrations!()`) but optimized for Postgres-specific features (replicas, advisory locks, SKIP LOCKED, LISTEN/NOTIFY). No shared abstraction layer between `modo-sqlite` and `modo-pg` — each is purpose-built. The `embed_migrations!()` macro may be extracted to a shared macro crate when `modo-pg` ships, or duplicated if the code is small enough.

## Testing

- `connect()` applies correct default PRAGMAs to all pool connections (acquire multiple, verify each)
- `connect_rw()` applies different PRAGMA values to reader vs writer pools
- `connect_rw()` writer pool enforces `max_connections: 1` even if `writer_max_connections` set higher
- `connect_rw()` with `:memory:` path returns an error
- Reader pool respects explicit `max_connections` config value
- `embed_migrations!()` discovers and embeds SQL files at compile time
- `embed_migrations!()` produces compile error for malformed filenames (non-14-digit timestamp, missing separator, non-numeric timestamp)
- `embed_migrations!()` produces compile error for duplicate versions in same directory
- Non-`.sql` files in migrations directory are ignored
- `run_migrations()` runs all groups
- `run_migrations_group()` runs only specified group
- `run_migrations_except()` excludes specified groups
- Duplicate migration versions within a group produce a runtime error
- `_modo_sqlite_migrations` table tracks group per migration
- `modo_sqlite::Error` variants map correctly from sqlx errors
- `From<modo_sqlite::Error> for modo::Error` produces correct HTTP status codes
- `ReadPool` cannot be passed to `run_migrations()` (compile-time check)
- Extractors return 500 when pool is not registered
- Parent directories are created for database path
- `:memory:` path works with `connect()` for in-memory databases
- Concurrent writes through `WritePool` are serialized (no SQLITE_BUSY)
