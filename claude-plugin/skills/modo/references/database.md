# Database (`modo::db`)

SQLite database layer built on raw sqlx. No ORM. One module: `src/db/`.

## Pool Newtypes

All three wrap `sqlx::SqlitePool` (aliased as `InnerPool`) and `Deref` to it, so you can pass `&*pool` directly to sqlx queries.

| Type        | Implements        | Purpose                                       |
| ----------- | ----------------- | --------------------------------------------- |
| `Pool`      | `Reader + Writer` | Single pool for both reads and writes         |
| `ReadPool`  | `Reader` only     | Read-only handle; prevents accidental writes  |
| `WritePool` | `Reader + Writer` | Write handle; defaults to `max_connections=1` |

Each has `::new(InnerPool) -> Self` and `Clone`.

## Reader / Writer Traits

```rust
pub trait Reader {
    fn read_pool(&self) -> &InnerPool;
}

pub trait Writer {
    fn write_pool(&self) -> &InnerPool;
}
```

Use these as bounds on functions that interact with the database:

```rust
pub async fn find_user(id: &str, db: &impl Reader) -> Result<User> {
    let row = sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = ?")
        .bind(id)
        .fetch_one(db.read_pool())
        .await?;
    Ok(row)
}

pub async fn create_user(user: &NewUser, db: &impl Writer) -> Result<()> {
    sqlx::query("INSERT INTO users (id, name) VALUES (?, ?)")
        .bind(&user.id)
        .bind(&user.name)
        .execute(db.write_pool())
        .await?;
    Ok(())
}
```

`ReadPool` intentionally does **not** implement `Writer`, so passing it to a write function is a compile error.

## Connection Functions

### `connect(config) -> Result<Pool>`

Opens a single connection pool. Use for simple apps or in-memory databases.

```rust
use modo::db::{self, SqliteConfig};

let config = SqliteConfig {
    path: "data/app.db".to_string(),
    ..Default::default()
};
let pool = db::connect(&config).await?;
```

### `connect_rw(config) -> Result<(ReadPool, WritePool)>`

Opens separate read and write pools. The read pool opens the file in read-only mode (`?mode=ro`). The write pool defaults to `max_connections=1` to serialize writes.

```rust
let (read_pool, write_pool) = db::connect_rw(&config).await?;
```

## Using sqlx Queries

All pool newtypes `Deref` to `sqlx::SqlitePool`, so you can use `&*pool` anywhere sqlx expects an executor:

```rust
// With Deref (common in handlers/tests)
let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
    .fetch_one(&*pool)
    .await?;

// With Reader/Writer traits (common in service functions)
let rows = sqlx::query_as::<_, UserRow>("SELECT * FROM users WHERE active = ?")
    .bind(true)
    .fetch_all(db.read_pool())
    .await?;
```

## sqlx Error Conversion

`sqlx::Error` converts automatically into `modo::Error` via `From`:

| sqlx error                  | HTTP status | message                 |
| --------------------------- | ----------- | ----------------------- |
| `RowNotFound`               | 404         | "record not found"      |
| unique constraint violation | 409         | "record already exists" |
| foreign key violation       | 400         | "foreign key violation" |
| `PoolTimedOut`              | 500         | "database pool timeout" |
| all others                  | 500         | "database error"        |

Use `?` in handlers and the conversion happens automatically.

## Migrations

```rust
db::migrate("migrations", &pool).await?;
```

Accepts any `&impl Writer`. Uses sqlx's standard migration file naming (e.g., `001_create_users.sql`).

## Managed Pool (Graceful Shutdown)

`ManagedPool` implements `Task`. Consume a pool into `ManagedPool` for use with the `run!` macro:

```rust
let managed = db::managed(pool.clone());
run!(server, managed);
```

Accepts `Pool`, `ReadPool`, or `WritePool` (each has `impl From<T> for ManagedPool`). **Consumes the pool** -- clone first if you need continued access. On shutdown, closes the pool and drains connections.

## Configuration (`SqliteConfig`)

`#[non_exhaustive]`. Loaded from YAML. `db::Config` is a type alias for `SqliteConfig`.

Key defaults:

| Field                  | Type                | Default                           |
| ---------------------- | ------------------- | --------------------------------- |
| `path`                 | `String`            | `"data/app.db"`                   |
| `max_connections`      | `u32`               | `10`                              |
| `min_connections`      | `u32`               | `1`                               |
| `journal_mode`         | `JournalMode`       | `Wal`                             |
| `synchronous`          | `SynchronousMode`   | `Normal`                          |
| `foreign_keys`         | `bool`              | `true`                            |
| `busy_timeout`         | `u64`               | `5000` ms                         |
| `cache_size`           | `i64`               | `-2000` (~2 MB)                   |
| `acquire_timeout_secs` | `u64`               | `30`                              |
| `idle_timeout_secs`    | `u64`               | `600`                             |
| `max_lifetime_secs`    | `u64`               | `1800`                            |
| `mmap_size`            | `Option<u64>`       | `None`                            |
| `temp_store`           | `Option<TempStore>` | `None`                            |
| `wal_autocheckpoint`   | `Option<u32>`       | `None`                            |
| `reader`               | `PoolOverrides`     | `PoolOverrides::default_reader()` |
| `writer`               | `PoolOverrides`     | `PoolOverrides::default_writer()` |

### Enum types

**`JournalMode`**: `Delete`, `Truncate`, `Persist`, `Memory`, `Wal`, `Off`.

**`SynchronousMode`**: `Off`, `Normal`, `Full`, `Extra`.

**`TempStore`**: `Default`, `File`, `Memory`.

All three are publicly exported from `modo::db`.

### PoolOverrides

`#[non_exhaustive]`. `reader` and `writer` fields on `SqliteConfig` hold `PoolOverrides` for `connect_rw()`. All fields are `Option<T>` (override the base `SqliteConfig` value when `Some`).

Fields: `max_connections`, `min_connections`, `acquire_timeout_secs`, `idle_timeout_secs`, `max_lifetime_secs`, `busy_timeout`, `cache_size`, `mmap_size`, `temp_store`, `wal_autocheckpoint`.

Constructor methods:

- `PoolOverrides::default_reader() -> Self` — returns a `PoolOverrides` pre-filled with reader defaults (`busy_timeout=1000`, `cache_size=-16000`, `mmap_size=268435456`).
- `PoolOverrides::default_writer() -> Self` — returns a `PoolOverrides` pre-filled with writer defaults (`max_connections=1`, `busy_timeout=2000`, `cache_size=-16000`, `mmap_size=268435456`).

Reader defaults: `busy_timeout=1000`, `cache_size=-16000` (~16 MB), `mmap_size=268435456` (256 MiB). Writer defaults: `max_connections=1`, `busy_timeout=2000`, `cache_size=-16000` (~16 MB), `mmap_size=268435456` (256 MiB).

## Registry Integration

Add pools to the service registry for extraction in handlers:

```rust
let mut registry = service::Registry::new();
registry.add(pool.clone());
// or for read/write split:
registry.add(read_pool.clone());
registry.add(write_pool.clone());
```

Extract in handlers via `Service<T>`:

```rust
async fn list_users(Service(pool): Service<ReadPool>) -> Result<Json<Vec<User>>> {
    let users = sqlx::query_as::<_, User>("SELECT * FROM users")
        .fetch_all(&*pool)
        .await?;
    Ok(Json(users))
}
```

## In-Memory Test Pattern

Use `TestDb` (behind `test-helpers` feature) or manually share one pool:

```rust
// With TestDb (preferred)
use modo::testing::TestDb;
let db = TestDb::new().await
    .exec("CREATE TABLE users (id TEXT PRIMARY KEY, name TEXT NOT NULL)")
    .await;
let pool = db.pool();
let read_pool = db.read_pool();   // shares same underlying connection
let write_pool = db.write_pool(); // shares same underlying connection

// Manual approach
let pool = db::connect(&SqliteConfig {
    path: ":memory:".to_string(),
    ..Default::default()
}).await?;
let read_pool = ReadPool::new((*pool).clone());
let write_pool = WritePool::new((*pool).clone());
```

## Gotchas

- **`:memory:` forces `max_connections=1`**: `connect()` auto-downgrades because each SQLite connection to `:memory:` gets its own isolated database instance. A pool with >1 connection would see different databases.

- **`connect_rw()` rejects `:memory:`**: Returns `Error::internal` immediately. In-memory databases cannot be shared across separate pools. Use `connect()` + `ReadPool::new()`/`WritePool::new()` to share one pool.

- **999 bind params limit**: SQLite limits a single statement to 999 bound parameters. Batch operations must chunk accordingly (e.g., max ~900 registered job handlers in the worker poll loop).

- **No `ON CONFLICT` with partial unique indexes**: SQLite does not support `ON CONFLICT` clauses on partial unique indexes. Use plain `INSERT` and catch `is_unique_violation()` from the sqlx database error.

- **Pool types lack `Debug`**: `Pool`, `ReadPool`, `WritePool` do not implement `Debug`. In tests, use `.err().unwrap()` instead of `.unwrap_err()` when asserting errors.

- **PRAGMAs applied per-connection**: All configured PRAGMAs (`journal_mode`, `synchronous`, `foreign_keys`, `busy_timeout`, `cache_size`, etc.) are set via `after_connect`, so every new connection in the pool gets them automatically.

- **Parent directories auto-created**: `connect()` and `connect_rw()` create missing parent directories for the database file path.
