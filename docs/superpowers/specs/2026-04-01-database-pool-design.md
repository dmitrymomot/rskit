# DatabasePool — Multi-Database Shard Management

Date: 2026-04-01

## Problem

Some tenants generate disproportionate database load. Isolating them into
separate SQLite files prevents noisy-neighbor effects while keeping the same
schema, config, and query patterns. The framework needs a connection manager
that lazily opens per-shard databases and caches them for reuse.

## Design

### New type: `DatabasePool`

A sharded connection cache in `src/db/pool.rs`. Wraps a default `Database`
(the main database) plus a `ShardedMap` of lazily-opened shard databases.

```
DatabasePool
├── default: Database              (opened at construction)
├── config: Config                 (shared PRAGMAs + migrations for all shards)
└── shards: ShardedMap<Database>   (Vec<RwLock<HashMap<String, Database>>>)
```

Follows the `Arc<Inner>` pattern. `Clone` is cheap (reference count increment).

### Single entry point

```rust
pub async fn conn(&self, shard: Option<&str>) -> Result<Database>
```

- `None` — returns Arc clone of default database. No lock, no async work.
- `Some("name")` — sharded read-lock lookup. On hit: Arc clone. On miss:
  opens `{base_path}/{name}.db`, applies PRAGMAs, runs migrations, caches
  under write-lock, returns Arc clone.

Application code uses one code path regardless of whether the tenant has a
dedicated shard:

```rust
async fn list_users(pool: State<DatabasePool>, tenant: Tenant) -> Result<Json<Vec<User>>> {
    let users: Vec<User> = pool.conn(tenant.db_shard.as_deref()).await?
        .conn()
        .query_all("SELECT id, name FROM users", ())
        .await?;
    Ok(Json(users))
}
```

`tenant.db_shard` is `Option<String>` in the tenant table. `None` means
default database, `Some("xyz")` means dedicated shard. The routing is
data-driven — handler code never branches.

### Config

`PoolConfig` is nested inside the existing `Config`:

```rust
pub struct Config {
    // ... existing fields unchanged (path, migrations, PRAGMAs)

    /// Optional pool configuration for multi-database sharding.
    #[serde(default)]
    pub pool: Option<PoolConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PoolConfig {
    /// Directory where shard databases are stored.
    /// Each shard creates `{base_path}/{shard_name}.db`.
    #[serde(default = "defaults::base_path")]
    pub base_path: String,

    /// Number of lock shards for the connection map (default 16).
    #[serde(default = "defaults::shard_count")]
    pub shard_count: usize,
}
```

YAML:

```yaml
database:
  path: "data/app.db"
  journal_mode: wal
  migrations: "migrations"
  pool:
    base_path: "data/shards"
    shard_count: 16
```

Shard databases inherit PRAGMAs and migrations from the parent `Config`.
Every shard is guaranteed the same schema as the main database.

### Construction

```rust
let pool = DatabasePool::new(&config).await?;
```

- Opens the default database via existing `connect(&config)`.
- Reads `config.pool` for pool settings. Returns an error if `pool` is `None`.
- No shard connections are opened at construction — all lazy.

### ShardedMap

Same `Vec<RwLock<HashMap>>` pattern used in `src/middleware/rate_limit.rs`:

```rust
const DEFAULT_SHARDS: usize = 16;

struct ShardedMap {
    shards: Vec<RwLock<HashMap<String, Database>>>,
}
```

- **Fast path (cache hit):** read-lock on one shard, HashMap lookup, Arc
  clone. Lock held for nanoseconds, never across `.await`.
- **Slow path (cache miss):** drop read-lock, open connection (async),
  acquire write-lock, double-check (another thread may have inserted),
  insert. Two concurrent misses for the same key both open a connection —
  last writer wins, extra connection is dropped. Harmless for SQLite and
  unlikely with a small number of shards.
- **No cleanup task:** shard connections are long-lived, unlike rate-limit
  buckets that expire.

### Shard database creation

Shard opening reuses the existing `connect()` function with a cloned
`Config` where `path` is set to `{base_path}/{name}.db`. Since `connect()`
already creates parent directories and auto-creates the `.db` file via
libsql, no special creation logic is needed.

First `conn(Some("name"))` call for a new shard:

1. Clones `Config`, sets `path` to `{base_path}/{name}.db`
2. Calls `connect(&shard_config)` — creates file, applies PRAGMAs, runs
   migrations
3. Caches the `Database` in the sharded map
4. Returns Arc clone

No explicit "create shard" step needed.

### Error handling

| Scenario | Behavior |
|----------|----------|
| `conn(None)` | Returns default Database. Infallible in practice. |
| `conn(Some("cached"))` | Read-lock + Arc clone. Infallible. |
| `conn(Some("new"))` | Opens DB, applies PRAGMAs, runs migrations. |
| DB file can't be created | `Error::internal("failed to open shard database: {name}")` with source |
| PRAGMA fails | Same error chain as `connect()` |
| Migration fails | Same error chain as `connect()` |

### Graceful shutdown

```rust
pub struct ManagedDatabasePool(DatabasePool);

impl Task for ManagedDatabasePool {
    async fn shutdown(self) -> Result<()> {
        drop(self.0);
        Ok(())
    }
}

pub fn managed_pool(pool: DatabasePool) -> ManagedDatabasePool {
    ManagedDatabasePool(pool)
}
```

Dropping `DatabasePool` drops the default and all cached shard handles.

### Testing

`TestPool` in `src/testing/pool.rs` (gated behind `test-helpers`):

```rust
let pool = TestPool::new().await;

// Default (in-memory):
pool.conn(None).await?.conn().query_one("SELECT ...", params).await?;

// Named shard (in-memory — no file I/O in tests):
pool.conn(Some("tenant_a")).await?.conn().query_one("SELECT ...", params).await?;
```

`TestPool` uses `:memory:` for all shard connections (overrides `base_path`).

### Public API

New exports from `src/db/mod.rs`:

```rust
pub use pool::{DatabasePool, PoolConfig, ManagedDatabasePool, managed_pool};
```

New files:
- `src/db/pool.rs` — `DatabasePool`, `PoolConfig`, `ShardedMap`,
  `ManagedDatabasePool`, `managed_pool`
- `src/testing/pool.rs` — `TestPool`

No changes to existing types. `Database`, `Config`, `connect()`, `ConnExt`,
`ConnQueryExt` remain untouched — except `Config` gains an optional `pool`
field.
