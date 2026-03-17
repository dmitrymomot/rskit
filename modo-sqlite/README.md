# modo-sqlite

Pure sqlx SQLite layer for the modo framework. Provides connection pool management with optional
read/write split, configurable SQLite PRAGMAs applied on every connection, embedded SQL migrations
discovered at compile time via `inventory`, and axum extractors for handler access.

## Usage

Add to `Cargo.toml`:

```toml
[dependencies]
modo-sqlite = { path = "../modo-sqlite" }  # or version = "0.3"
```

### Single pool

```rust
use modo_sqlite::{SqliteConfig, connect, run_migrations};

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

Access the pool in a handler via the `Db` extractor:

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

### Read/write split

```rust
use modo_sqlite::{SqliteConfig, connect_rw, run_migrations};

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

Use `DbReader` and `DbWriter` extractors in handlers:

```rust
use modo_sqlite::{DbReader, DbWriter};

#[modo::handler(POST, "/items")]
async fn create_item(DbWriter(db): DbWriter, /* ... */) -> modo::JsonResult<Item> {
    // write through writer pool
    # todo!()
}

#[modo::handler(GET, "/items")]
async fn list_items(DbReader(db): DbReader) -> modo::JsonResult<Vec<Item>> {
    // read through reader pool
    # todo!()
}
```

## Migrations

Place SQL files under `migrations/` with names following the pattern
`{YYYYMMDDHHmmss}_{description}.sql`. Call `embed_migrations!()` once in your crate to register
all files at compile time.

```rust
// src/main.rs or any module that is linked into the binary
modo_sqlite::embed_migrations!();

// With a custom directory and group name:
modo_sqlite::embed_migrations!(path = "db/migrations", group = "jobs");
```

At startup, run the migrations:

```rust
// All groups:
modo_sqlite::run_migrations(&pool).await?;

// One group only:
modo_sqlite::run_migrations_group(&pool, "jobs").await?;

// All groups except listed ones:
modo_sqlite::run_migrations_except(&pool, &["jobs"]).await?;
```

Migrations are applied in version order, inside individual transactions, and are idempotent —
already-executed versions are skipped. Duplicate version numbers across all groups are detected
before any SQL runs and return an error.

## Configuration

`SqliteConfig` can be deserialized from YAML:

```yaml
sqlite:
  path: "data/app.db"          # default: "data/app.db"
  max_connections: 10           # default: 10
  min_connections: 1            # default: 1
  acquire_timeout_secs: 30      # default: 30
  idle_timeout_secs: 600        # default: 600
  max_lifetime_secs: 1800       # default: 1800
  journal_mode: WAL             # WAL | DELETE | TRUNCATE | PERSIST | OFF
  busy_timeout: 5000            # milliseconds
  synchronous: NORMAL           # FULL | NORMAL | OFF
  foreign_keys: true
  cache_size: -2000             # negative = KiB
  # mmap_size: 268435456        # optional, bytes
  # temp_store: MEMORY          # DEFAULT | FILE | MEMORY
  # wal_autocheckpoint: 1000    # optional, pages
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

`reader` and `writer` sections accept the same fields as `PoolOverrides` — all optional, they
override the top-level values for their respective pool only.

## Key Types

| Type | Description |
|------|-------------|
| `SqliteConfig` | Full connection pool configuration with PRAGMA settings |
| `PoolOverrides` | Per-pool overrides for read/write split setups |
| `JournalMode` | `Wal` (default), `Delete`, `Truncate`, `Persist`, `Off` |
| `SynchronousMode` | `Normal` (default), `Full`, `Off` |
| `TempStore` | `Default`, `File`, `Memory` |
| `Pool` | General-purpose (read + write) pool; implements `AsPool` |
| `ReadPool` | Read-only pool; does **not** implement `AsPool` |
| `WritePool` | Write-only pool; implements `AsPool` |
| `AsPool` | Trait required by `run_migrations*` — only writable pools satisfy it |
| `Db` | Axum extractor for `Pool` |
| `DbReader` | Axum extractor for `ReadPool` |
| `DbWriter` | Axum extractor for `WritePool` |
| `MigrationRegistration` | Compile-time migration entry populated by `embed_migrations!` |
| `Error` | Crate error type; converts from `sqlx::Error` and into `modo::Error` |

## ID Helpers

Two ID generators are provided:

```rust
let ulid = modo_sqlite::generate_ulid();       // 26-char Crockford Base32
let sid  = modo_sqlite::generate_short_id();   // 13-char Base36 [0-9a-z]
```

Prefer `generate_ulid()` for primary keys. Use `generate_short_id()` for human-readable IDs
where shorter length matters and collision risk at high throughput is acceptable.
