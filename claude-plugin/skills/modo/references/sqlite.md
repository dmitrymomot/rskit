# modo-sqlite Reference

`modo-sqlite` is the pure-sqlx SQLite layer for modo. It manages connection pools, applies
SQLite PRAGMAs on every connection, and runs embedded SQL migrations discovered at compile time
via `inventory`.

This is a separate crate from `modo-db` (which is the SeaORM layer). Use `modo-sqlite` when you
want to write raw sqlx queries or need fine-grained control over the SQLite connection.

---

## Setup

### Cargo.toml

```toml
[dependencies]
modo-sqlite = { path = "../modo-sqlite" }  # or version = "0.3"
```

### Single pool

```rust
#[modo::main]
async fn main(
    app: modo::app::AppBuilder,
    config: Config,
) -> Result<(), Box<dyn std::error::Error>> {
    let pool = modo_sqlite::connect(&config.sqlite).await?;
    modo_sqlite::run_migrations(&pool).await?;
    app.config(config.core).managed_service(pool).run().await
}
```

### Read/write split

```rust
#[modo::main]
async fn main(
    app: modo::app::AppBuilder,
    config: Config,
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

---

## Extractors

| Extractor | Pool type | Registration |
|-----------|-----------|-------------|
| `Db` | `Pool` | `app.managed_service(pool)` |
| `DbReader` | `ReadPool` | `app.managed_service(reader)` |
| `DbWriter` | `WritePool` | `app.managed_service(writer)` |

All extractors are in `modo_sqlite` — use them directly in handler parameters:

```rust
use modo_sqlite::Db;

#[modo::handler(GET, "/items")]
async fn list_items(Db(db): Db) -> modo::JsonResult<Vec<Item>> {
    let items = sqlx::query_as::<_, Item>("SELECT * FROM items")
        .fetch_all(db.pool())
        .await?;
    Ok(modo::Json(items))
}
```

For read/write split:

```rust
use modo_sqlite::{DbReader, DbWriter};

#[modo::handler(GET, "/items")]
async fn list_items(DbReader(db): DbReader) -> modo::JsonResult<Vec<Item>> {
    let items = sqlx::query_as::<_, Item>("SELECT * FROM items")
        .fetch_all(db.pool())
        .await?;
    Ok(modo::Json(items))
}

#[modo::handler(POST, "/items")]
async fn create_item(DbWriter(db): DbWriter, /* ... */) -> modo::JsonResult<Item> {
    // write queries through writer pool
    # todo!()
}
```

---

## Migrations

### Embedding SQL files

Place SQL files in `migrations/` (relative to the crate root) named:

```
{YYYYMMDDHHmmss}_{description}.sql
```

Call `embed_migrations!()` once anywhere that is linked into the binary. If the migrations
directory does not exist, the macro silently expands to nothing (no registrations, no error):

```rust
// Default directory ("migrations/"), default group ("default")
modo_sqlite::embed_migrations!();

// Custom directory and group
modo_sqlite::embed_migrations!(path = "db/migrations", group = "jobs");
```

### Running migrations

```rust
// Run all groups
modo_sqlite::run_migrations(&pool).await?;

// Run a specific group only
modo_sqlite::run_migrations_group(&pool, "jobs").await?;

// Run all groups except listed ones
modo_sqlite::run_migrations_except(&pool, &["jobs"]).await?;
```

Key guarantees:
- Each migration runs inside its own transaction.
- Already-executed versions are skipped (idempotent).
- Duplicate version numbers across all groups are detected before any SQL runs.
- `ReadPool` does not satisfy `AsPool` — migrations cannot be accidentally run through a read-only pool.

### Tracking table

Executed migrations are recorded in `_modo_sqlite_migrations`:

```sql
CREATE TABLE _modo_sqlite_migrations (
    version     BIGINT PRIMARY KEY,
    description TEXT NOT NULL,
    executed_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
)
```

---

## Configuration

`SqliteConfig` fields:

| Field | Type | Default |
|-------|------|---------|
| `path` | `String` | `"data/app.db"` |
| `max_connections` | `u32` | `10` |
| `min_connections` | `u32` | `1` |
| `acquire_timeout_secs` | `u64` | `30` |
| `idle_timeout_secs` | `u64` | `600` |
| `max_lifetime_secs` | `u64` | `1800` |
| `journal_mode` | `JournalMode` | `WAL` |
| `busy_timeout` | `u32` (ms) | `5000` |
| `synchronous` | `SynchronousMode` | `NORMAL` |
| `foreign_keys` | `bool` | `true` |
| `cache_size` | `i32` (neg = KiB) | `-2000` |
| `mmap_size` | `Option<i64>` (bytes) | `None` |
| `temp_store` | `Option<TempStore>` | `None` |
| `wal_autocheckpoint` | `Option<u32>` (pages) | `None` |
| `reader` | `PoolOverrides` | optimized for reads |
| `writer` | `PoolOverrides` | single connection |

Example YAML:

```yaml
sqlite:
  path: "data/app.db"
  max_connections: 10
  journal_mode: WAL
  busy_timeout: 5000
  synchronous: NORMAL
  foreign_keys: true
  cache_size: -2000
  reader:
    busy_timeout: 1000
    cache_size: -16000
    mmap_size: 268435456
  writer:
    max_connections: 1
    busy_timeout: 2000
    cache_size: -16000
    mmap_size: 268435456
```

`PoolOverrides` accepts the same fields with `Option<T>` types — only set fields override
the top-level value.

---

## Error Handling

`modo_sqlite::Error` converts automatically from `sqlx::Error` and into `modo::Error`:

| Variant | HTTP status |
|---------|------------|
| `NotFound` | 404 Not Found |
| `UniqueViolation(String)` | 409 Conflict |
| `ForeignKeyViolation(String)` | 400 Bad Request |
| `PoolTimeout` | 500 Internal Server Error |
| `Query(sqlx::Error)` | 500 Internal Server Error |

Use the `?` operator in handlers — `modo_sqlite::Error` converts to `modo::Error` automatically.

---

## ID Helpers

```rust
let ulid = modo_sqlite::generate_ulid();       // 26-char Crockford Base32
let sid  = modo_sqlite::generate_short_id();   // 13-char Base36 [0-9a-z]
```

Prefer `generate_ulid()` for primary keys. Use `generate_short_id()` when shorter IDs are
needed and lower randomness (22 bits, ~4M/ms) is acceptable.

---

## Key Type Reference

| Type | Path |
|------|------|
| `SqliteConfig` | `modo_sqlite::SqliteConfig` |
| `PoolOverrides` | `modo_sqlite::PoolOverrides` |
| `JournalMode` | `modo_sqlite::JournalMode` |
| `SynchronousMode` | `modo_sqlite::SynchronousMode` |
| `TempStore` | `modo_sqlite::TempStore` |
| `Pool` | `modo_sqlite::Pool` |
| `ReadPool` | `modo_sqlite::ReadPool` |
| `WritePool` | `modo_sqlite::WritePool` |
| `AsPool` | `modo_sqlite::AsPool` |
| `Db` | `modo_sqlite::Db` |
| `DbReader` | `modo_sqlite::DbReader` |
| `DbWriter` | `modo_sqlite::DbWriter` |
| `MigrationRegistration` | `modo_sqlite::MigrationRegistration` |
| `Error` | `modo_sqlite::Error` |
| `connect` | `modo_sqlite::connect` |
| `connect_rw` | `modo_sqlite::connect_rw` |
| `run_migrations` | `modo_sqlite::run_migrations` |
| `run_migrations_group` | `modo_sqlite::run_migrations_group` |
| `run_migrations_except` | `modo_sqlite::run_migrations_except` |
| `embed_migrations!` | `modo_sqlite::embed_migrations` |
| `generate_ulid` | `modo_sqlite::generate_ulid` |
| `generate_short_id` | `modo_sqlite::generate_short_id` |
