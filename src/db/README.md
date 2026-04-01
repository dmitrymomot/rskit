# db

Lightweight libsql (SQLite) database layer with typed row mapping,
composable query building, filtering, and pagination.

```toml
[dependencies]
modo = { version = "...", features = ["db"] }
```

## Key types

| Type               | Purpose                                                                  |
| ------------------ | ------------------------------------------------------------------------ |
| `Database`         | Clone-able, Arc-wrapped single-connection handle                         |
| `Config`           | YAML-deserializable database configuration with PRAGMA defaults          |
| `connect`          | Opens a database, applies PRAGMAs, optionally runs migrations            |
| `migrate`          | Runs `*.sql` migrations with checksum tracking                           |
| `ManagedDatabase`  | Wrapper for graceful shutdown via `modo::run!`                           |
| `managed`          | Wraps a `Database` into a `ManagedDatabase`                              |
| `DatabasePool`     | Multi-database pool with lazy shard opening for tenant isolation         |
| `PoolConfig`       | Configuration for database sharding (base_path, shard_count)             |
| `ManagedDatabasePool` | Wrapper for graceful pool shutdown via `modo::run!`                   |
| `managed_pool`     | Wraps a `DatabasePool` into a `ManagedDatabasePool`                      |
| `ConnExt`          | Low-level `query_raw`/`execute_raw` trait for Connection and Transaction |
| `ConnQueryExt`     | High-level `query_one`/`query_all`/`query_optional` + `_map` variants    |
| `SelectBuilder`    | Composable query builder with filter + sort + pagination                 |
| `FromRow`          | Trait for converting a `libsql::Row` into a Rust struct                  |
| `FromValue`        | Trait for converting a `libsql::Value` into a concrete type              |
| `ColumnMap`        | Column name-to-index lookup for name-based row access                    |
| `Filter`           | Raw parsed filter from query string (axum extractor)                     |
| `FilterSchema`     | Declares allowed filter and sort fields for an endpoint                  |
| `ValidatedFilter`  | Schema-validated filter safe for SQL generation                          |
| `FieldType`        | Column type enum for filter value validation                             |
| `PageRequest`      | Offset pagination extractor (`?page=N&per_page=N`)                       |
| `Page`             | Offset page response with total/has_next/has_prev                        |
| `CursorRequest`    | Cursor pagination extractor (`?after=<cursor>&per_page=N`)               |
| `CursorPage`       | Cursor page response with next_cursor/has_more                           |
| `PaginationConfig` | Configurable defaults and limits for pagination extractors               |
| `JournalMode`      | SQLite journal mode enum (WAL, Delete, Truncate, Memory, Off)            |
| `SynchronousMode`  | SQLite synchronous mode enum (Off, Normal, Full, Extra)                  |
| `TempStore`        | SQLite temp store location enum (Default, File, Memory)                  |

The `libsql` crate is also re-exported for direct access to low-level types
like `libsql::params!`, `libsql::Value`, `libsql::Connection`, and
`libsql::Transaction`.

## Usage

### Connecting

```rust,no_run
use modo::db;

let config = db::Config::default();
// Default: data/app.db, WAL mode, foreign keys on,
// busy_timeout=5000ms, cache_size=16384KB, mmap_size=256MB

let db = db::connect(&config).await?;
```

### Managed shutdown

```rust,no_run
use modo::db;

let db = db::connect(&db::Config::default()).await?;
let task = db::managed(db.clone());
// Register `task` with modo::run!() for graceful shutdown
```

### DatabasePool (multi-database sharding)

`DatabasePool` manages a default database plus lazily-opened shard databases.
Use `conn(None)` for the default database and `conn(Some("shard_name"))` for
a tenant shard.

```rust,no_run
use modo::db::{self, ConnExt, DatabasePool};

let config = db::Config {
    path: "data/app.db".to_string(),
    migrations: Some("migrations".to_string()),
    pool: Some(db::PoolConfig {
        base_path: "data/shards".to_string(),
        shard_count: 16,
    }),
    ..Default::default()
};

let pool = DatabasePool::new(&config).await?;

// Default database
let db = pool.conn(None).await?;
db.conn().execute_raw("SELECT 1", ()).await?;

// Tenant shard (lazy open + cache)
let shard = pool.conn(Some("tenant_abc")).await?;
shard.conn().execute_raw("SELECT 1", ()).await?;

// Graceful shutdown
let task = db::managed_pool(pool.clone());
// Register `task` with modo::run!()
```

Shard databases are created at `{base_path}/{shard_name}.db` and inherit all
PRAGMAs and migrations from the parent config. Connections are cached after
first open.

**Memory note:** Each shard connection uses its own SQLite page cache
(default `cache_size` = 16 MB) and mmap region (default `mmap_size` = 256 MB
virtual). At 100 shards this means up to ~1.6 GB of page cache memory. For
large shard counts, consider lowering `cache_size` in the config to reduce
per-connection memory.

### Implementing FromRow

```rust,no_run
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

### Querying with ConnQueryExt

```rust,no_run
use modo::db::ConnQueryExt;

// Single row (returns Error::not_found if missing)
let user: User = db.conn().query_one(
    "SELECT id, name, age FROM users WHERE id = ?1",
    libsql::params!["user_abc"],
).await?;

// Optional row
let maybe_user: Option<User> = db.conn().query_optional(
    "SELECT id, name, age FROM users WHERE id = ?1",
    libsql::params!["user_abc"],
).await?;

// All matching rows
let users: Vec<User> = db.conn().query_all(
    "SELECT id, name, age FROM users ORDER BY name",
    (),
).await?;
```

### Querying with closures (\_map variants)

```rust,no_run
use modo::db::{ConnQueryExt, ColumnMap};

// Map rows with a closure instead of implementing FromRow
let name: String = db.conn().query_one_map(
    "SELECT name FROM users WHERE id = ?1",
    libsql::params!["user_abc"],
    |row| {
        let cols = ColumnMap::from_row(row);
        cols.get::<String>(row, "name")
    },
).await?;

// Also available: query_optional_map, query_all_map
```

### SelectBuilder with filtering and pagination

```rust,no_run
use modo::db::{self, ConnExt, Filter, FilterSchema, FieldType, PageRequest};

async fn list_users(
    filter: Filter,
    page_req: PageRequest,
    db: axum::Extension<db::Database>,
) -> modo::Result<axum::Json<db::Page<User>>> {
    let schema = FilterSchema::new()
        .field("name", FieldType::Text)
        .field("age", FieldType::Int)
        .sort_fields(&["name", "age"]);

    let validated = filter.validate(&schema)?;

    let page = db.conn()
        .select("SELECT id, name, age FROM users")
        .filter(validated)
        .order_by("\"created_at\" DESC")
        .page::<User>(page_req)
        .await?;

    Ok(axum::Json(page))
}
```

### Cursor pagination

```rust,no_run
use modo::db::{ConnExt, CursorRequest};

let cursor_page = db.conn()
    .select("SELECT id, name FROM users")
    .cursor_column("id")    // default is "id"
    .oldest_first()          // default is newest-first (DESC)
    .cursor::<User>(cursor_req)
    .await?;
// cursor_page.next_cursor contains the cursor for the next page
```

### Migrations

```rust,no_run
use modo::db;

// Migrations run automatically if Config::migrations is set:
let config = db::Config {
    migrations: Some("migrations".to_string()),
    ..Default::default()
};
let db = db::connect(&config).await?;

// Or run manually against a connection:
db::migrate(db.conn(), "migrations").await?;
```

Migration files are `*.sql` files in the directory, sorted by filename.
Each migration is tracked in a `_migrations` table with a checksum.
Modified files after application produce an error.

## Configuration

```yaml
database:
    path: "data/app.db"
    migrations: "migrations" # optional — run on connect
    busy_timeout: 5000 # ms
    cache_size: 16384 # KB (PRAGMA cache_size = -N)
    mmap_size: 268435456 # bytes (256 MB)
    journal_mode: wal # wal | delete | truncate | memory | off
    synchronous: normal # off | normal | full | extra
    foreign_keys: true
    temp_store: memory # default | file | memory
    pool: # optional — enables DatabasePool
        base_path: "data/shards" # directory for shard .db files
        shard_count: 16 # number of lock shards for the connection map
```

## Filter query string syntax

| Pattern                          | SQL                              |
| -------------------------------- | -------------------------------- |
| `?status=active`                 | `WHERE "status" = ?`             |
| `?status=active&status=inactive` | `WHERE "status" IN (?, ?)`       |
| `?age.gt=18`                     | `WHERE "age" > ?`                |
| `?age.gte=18`                    | `WHERE "age" >= ?`               |
| `?age.lt=65`                     | `WHERE "age" < ?`                |
| `?age.lte=65`                    | `WHERE "age" <= ?`               |
| `?name.like=%smith%`             | `WHERE "name" LIKE ?`            |
| `?name.ne=admin`                 | `WHERE "name" != ?`              |
| `?deleted_at.null=true`          | `WHERE "deleted_at" IS NULL`     |
| `?deleted_at.null=false`         | `WHERE "deleted_at" IS NOT NULL` |
| `?sort=name`                     | `ORDER BY "name" ASC`            |
| `?sort=-name`                    | `ORDER BY "name" DESC`           |

Unknown fields and operators are silently ignored. Type mismatches
return a 400 error.

## Error handling

`libsql::Error` is automatically converted to `modo::Error` with
appropriate HTTP status codes:

| SQLite error                  | HTTP status               |
| ----------------------------- | ------------------------- |
| Unique/primary key constraint | 409 Conflict              |
| Foreign key violation         | 400 Bad Request           |
| Query returned no rows        | 404 Not Found             |
| Connection failure            | 500 Internal Server Error |
| Other errors                  | 500 Internal Server Error |
