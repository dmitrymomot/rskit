# modo::db

SQLite database layer for the modo web framework. Provides connection pooling,
migration support, and type-safe pool wrappers built on top of
[sqlx](https://crates.io/crates/sqlx).

## Key Types

| Type / Trait      | Purpose                                                               |
| ----------------- | --------------------------------------------------------------------- |
| `SqliteConfig`    | Pool configuration (path, limits, PRAGMAs). Also aliased as `Config`. |
| `Pool`            | Single pool for both reads and writes.                                |
| `ReadPool`        | Read-only handle; does not implement `Writer`.                        |
| `WritePool`       | Write-capable handle; defaults to one connection.                     |
| `Reader`          | Trait providing `read_pool() -> &InnerPool`.                          |
| `Writer`          | Trait providing `write_pool() -> &InnerPool`.                         |
| `ManagedPool`     | Pool wrapper that implements `Task` for graceful shutdown.            |
| `JournalMode`     | SQLite `PRAGMA journal_mode` enum.                                    |
| `SynchronousMode` | SQLite `PRAGMA synchronous` enum.                                     |
| `TempStore`       | SQLite `PRAGMA temp_store` enum.                                      |
| `PoolOverrides`   | Per-pool PRAGMA overrides for read/write split configurations.        |

## Usage

### Single pool (simple apps)

```rust
use modo::db::{self, SqliteConfig};

let config = SqliteConfig {
    path: "data/app.db".to_string(),
    ..Default::default()
};

let pool = db::connect(&config).await?;
db::migrate("migrations", &pool).await?;

// Pool derefs to sqlx::SqlitePool for direct query use.
let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
    .fetch_one(&*pool)
    .await?;
```

### Read/write split

```rust
use modo::db::{self, SqliteConfig};

let config = SqliteConfig {
    path: "data/app.db".to_string(),
    ..Default::default()
};

let (reader, writer) = db::connect_rw(&config).await?;
db::migrate("migrations", &writer).await?;

// writer: serialized writes (max_connections = 1 by default)
sqlx::query("INSERT INTO users (id, name) VALUES (?, ?)")
    .bind("01HX...")
    .bind("Alice")
    .execute(&*writer)
    .await?;

// reader: concurrent reads
let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
    .fetch_one(&*reader)
    .await?;
```

### In-memory databases (tests)

`connect_rw` rejects `:memory:` because each SQLite connection gets its own
isolated database. Share a single pool via `ReadPool::new` / `WritePool::new`:

```rust
use modo::db::{self, ReadPool, SqliteConfig, WritePool};

let config = SqliteConfig {
    path: ":memory:".to_string(),
    ..Default::default()
};

let pool = db::connect(&config).await?;
let reader = ReadPool::new((*pool).clone());
let writer = WritePool::new((*pool).clone());
```

### Graceful shutdown

Wrap a pool with `managed` to participate in the `modo::run!` shutdown sequence:

```rust
use modo::db::{self, SqliteConfig};

let pool = db::connect(&config).await?;
let managed = db::managed(pool.clone());

// pool is still usable after this; managed is consumed by the shutdown sequence
modo::run!(server, managed).await
```

`managed` accepts `Pool`, `ReadPool`, or `WritePool`. For a read/write split,
wrap each pool separately.

## Configuration

`SqliteConfig` deserializes from YAML. All fields are optional — unset fields
fall back to the defaults shown below.

```yaml
database:
    path: data/app.db # default: "data/app.db"
    max_connections: 10 # default: 10
    min_connections: 1 # default: 1
    acquire_timeout_secs: 30 # default: 30
    idle_timeout_secs: 600 # default: 600
    max_lifetime_secs: 1800 # default: 1800
    journal_mode: WAL # DELETE | TRUNCATE | PERSIST | MEMORY | WAL | OFF
    synchronous: NORMAL # OFF | NORMAL | FULL | EXTRA
    foreign_keys: true # default: true
    busy_timeout: 5000 # milliseconds, default: 5000
    cache_size: -2000 # KiB when negative; default: -2000 (2 MB)
    # mmap_size: 268435456       # bytes, optional
    # temp_store: MEMORY         # DEFAULT | FILE | MEMORY, optional
    # wal_autocheckpoint: 1000   # pages, optional

    # Fine-tune read pool (used by connect_rw)
    reader:
        busy_timeout: 1000
        cache_size: -16000
        mmap_size: 268435456

    # Fine-tune write pool (used by connect_rw)
    writer:
        max_connections: 1
        busy_timeout: 2000
        cache_size: -16000
        mmap_size: 268435456
```

## Error mapping

`sqlx::Error` is automatically converted to `modo::Error` with appropriate
HTTP status codes:

| sqlx error                  | HTTP status | message                 |
| --------------------------- | ----------- | ----------------------- |
| `RowNotFound`               | 404         | "record not found"      |
| unique constraint violation | 409         | "record already exists" |
| foreign key violation       | 400         | "foreign key violation" |
| `PoolTimedOut`              | 500         | "database pool timeout" |
| all others                  | 500         | "database error"        |
