# DatabasePool Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a `DatabasePool` type that manages a default database plus lazily-opened shard databases, enabling tenant isolation with a single unified `conn(shard: Option<&str>)` API.

**Architecture:** New `DatabasePool` type in `src/db/pool.rs` wraps a default `Database` plus a `ShardedMap` (sharded `RwLock<HashMap>`) of cached shard connections. `PoolConfig` nests inside the existing `Config`. Shard databases reuse `connect()` with a modified path, inheriting PRAGMAs and migrations. `TestPool` in `src/testing/pool.rs` uses in-memory databases for tests.

**Tech Stack:** Rust 2024, libsql, `std::sync::RwLock`, `std::hash::DefaultHasher`, existing `modo::db` module.

**Spec:** `docs/superpowers/specs/2026-04-01-database-pool-design.md`

---

## File Structure

| Action | Path | Responsibility |
|--------|------|----------------|
| Modify | `src/db/config.rs` | Add `PoolConfig` and nest it in `Config` |
| Create | `src/db/pool.rs` | `DatabasePool`, `ShardedMap`, `ManagedDatabasePool`, `managed_pool` |
| Modify | `src/db/mod.rs` | Add `mod pool` and public re-exports |
| Create | `src/testing/pool.rs` | `TestPool` for in-memory shard testing |
| Modify | `src/testing/mod.rs` | Add `mod pool` and re-export `TestPool` |
| Create | `tests/db_pool_test.rs` | Integration tests for `DatabasePool` |

---

### Task 1: Add `PoolConfig` to `Config`

**Files:**
- Modify: `src/db/config.rs`
- Test: `tests/db_pool_test.rs`

- [ ] **Step 1: Write the failing test**

Create `tests/db_pool_test.rs`:

```rust
#![cfg(feature = "db")]

use modo::db;

#[test]
fn config_pool_defaults_to_none() {
    let config = db::Config::default();
    assert!(config.pool.is_none());
}

#[test]
fn config_pool_deserializes_from_yaml() {
    let yaml = r#"
path: "data/app.db"
pool:
  base_path: "data/shards"
  shard_count: 8
"#;
    let config: db::Config = serde_yaml_ng::from_str(yaml).unwrap();
    let pool = config.pool.unwrap();
    assert_eq!(pool.base_path, "data/shards");
    assert_eq!(pool.shard_count, 8);
}

#[test]
fn pool_config_defaults() {
    let yaml = r#"
path: "data/app.db"
pool: {}
"#;
    let config: db::Config = serde_yaml_ng::from_str(yaml).unwrap();
    let pool = config.pool.unwrap();
    assert_eq!(pool.base_path, "data/shards");
    assert_eq!(pool.shard_count, 16);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --features db --test db_pool_test -- --nocapture`
Expected: FAIL — `Config` has no `pool` field, `PoolConfig` not exported.

- [ ] **Step 3: Add `PoolConfig` struct and nest in `Config`**

In `src/db/config.rs`, add after the `TempStore` impl block (after line 140):

```rust
/// Pool configuration for multi-database sharding.
///
/// When nested inside [`Config`], enables [`DatabasePool`](super::DatabasePool)
/// to manage lazily-opened shard databases that share the parent config's
/// PRAGMAs and migrations.
#[derive(Debug, Clone, Deserialize)]
pub struct PoolConfig {
    /// Directory where shard databases are stored.
    /// Each shard creates `{base_path}/{shard_name}.db`.
    #[serde(default = "defaults::base_path")]
    pub base_path: String,

    /// Number of lock shards for the connection map.
    #[serde(default = "defaults::shard_count")]
    pub shard_count: usize,
}
```

Add to the `Config` struct (after the `temp_store` field):

```rust
    /// Optional pool configuration for multi-database sharding.
    /// When set, [`DatabasePool::new`](super::DatabasePool::new) can be used
    /// to manage shard databases that share this config's PRAGMAs and migrations.
    #[serde(default)]
    pub pool: Option<PoolConfig>,
```

Add to `Config::default()` impl:

```rust
            pool: None,
```

Add to the `defaults` module:

```rust
    pub fn base_path() -> String {
        "data/shards".to_string()
    }

    pub fn shard_count() -> usize {
        16
    }
```

- [ ] **Step 4: Export `PoolConfig` from `src/db/mod.rs`**

Change the config re-export line in `src/db/mod.rs` from:

```rust
pub use config::{Config, JournalMode, SynchronousMode, TempStore};
```

to:

```rust
pub use config::{Config, JournalMode, PoolConfig, SynchronousMode, TempStore};
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --features db --test db_pool_test -- --nocapture`
Expected: All 3 tests PASS.

- [ ] **Step 6: Run existing db tests to verify no regressions**

Run: `cargo test --features db --test db_test -- --nocapture`
Expected: All existing tests PASS.

- [ ] **Step 7: Commit**

```
git add src/db/config.rs src/db/mod.rs tests/db_pool_test.rs
git commit -m "feat(db): add PoolConfig nested in Config"
```

---

### Task 2: Implement `ShardedMap`

**Files:**
- Create: `src/db/pool.rs`
- Modify: `src/db/mod.rs`

- [ ] **Step 1: Write the failing test**

Add to `src/db/pool.rs` (the test module will be at the bottom — we write the test first, then the implementation above it):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_db() -> Database {
        // Directly construct a Database for unit testing the map.
        // We only need it as a value in the map — no real queries.
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let config = super::super::config::Config {
                path: ":memory:".to_string(),
                ..Default::default()
            };
            super::super::connect::connect(&config).await.unwrap()
        })
    }

    #[test]
    fn sharded_map_get_returns_none_for_missing_key() {
        let map = ShardedMap::new(4);
        assert!(map.get("missing").is_none());
    }

    #[test]
    fn sharded_map_insert_and_get() {
        let map = ShardedMap::new(4);
        let db = make_test_db();
        map.insert("tenant_a".to_string(), db);
        assert!(map.get("tenant_a").is_some());
    }

    #[test]
    fn sharded_map_different_keys_independent() {
        let map = ShardedMap::new(4);
        let db = make_test_db();
        map.insert("tenant_a".to_string(), db);
        assert!(map.get("tenant_a").is_some());
        assert!(map.get("tenant_b").is_none());
    }

    #[test]
    fn sharded_map_insert_idempotent() {
        let map = ShardedMap::new(4);
        let db1 = make_test_db();
        let db2 = make_test_db();
        map.insert("key".to_string(), db1);
        map.insert("key".to_string(), db2);
        assert!(map.get("key").is_some());
    }
}
```

- [ ] **Step 2: Write `ShardedMap` implementation**

Write the top of `src/db/pool.rs`:

```rust
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::RwLock;

use super::database::Database;

// ---------------------------------------------------------------------------
// Sharded map
// ---------------------------------------------------------------------------

const DEFAULT_SHARDS: usize = 16;

struct ShardedMap {
    shards: Vec<RwLock<HashMap<String, Database>>>,
}

impl ShardedMap {
    fn new(num_shards: usize) -> Self {
        let mut shards = Vec::with_capacity(num_shards);
        for _ in 0..num_shards {
            shards.push(RwLock::new(HashMap::new()));
        }
        Self { shards }
    }

    fn shard_index(&self, key: &str) -> usize {
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        hasher.finish() as usize % self.shards.len()
    }

    /// Look up a cached `Database` by key. Returns a clone (cheap Arc bump)
    /// or `None` if the key is not present.
    fn get(&self, key: &str) -> Option<Database> {
        let idx = self.shard_index(key);
        let shard = &self.shards[idx];
        let read = shard.read().expect("pool shard lock poisoned");
        read.get(key).cloned()
    }

    /// Insert a `Database` under `key`. If the key already exists the old
    /// value is replaced (last writer wins).
    fn insert(&self, key: String, db: Database) {
        let idx = self.shard_index(&key);
        let shard = &self.shards[idx];
        let mut write = shard.write().expect("pool shard lock poisoned");
        write.insert(key, db);
    }
}
```

- [ ] **Step 3: Add the module declaration to `src/db/mod.rs`**

Add after the `managed` module lines:

```rust
mod pool;
```

(No public re-exports yet — we'll add them in Task 3.)

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --features db -- db::pool::tests --nocapture`
Expected: All 4 `ShardedMap` tests PASS.

- [ ] **Step 5: Commit**

```
git add src/db/pool.rs src/db/mod.rs
git commit -m "feat(db): add ShardedMap for connection caching"
```

---

### Task 3: Implement `DatabasePool`

**Files:**
- Modify: `src/db/pool.rs`
- Modify: `src/db/mod.rs`
- Modify: `tests/db_pool_test.rs`

- [ ] **Step 1: Write the failing integration tests**

Append to `tests/db_pool_test.rs`:

```rust
#[tokio::test]
async fn pool_new_fails_without_pool_config() {
    let config = db::Config::default(); // pool is None
    let result = db::DatabasePool::new(&config).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn pool_conn_none_returns_default() {
    let config = db::Config {
        path: ":memory:".to_string(),
        pool: Some(db::PoolConfig {
            base_path: "data/test_shards".to_string(),
            shard_count: 4,
        }),
        ..Default::default()
    };
    let pool = db::DatabasePool::new(&config).await.unwrap();

    // conn(None) returns the default database — verify it works
    let db = pool.conn(None).await.unwrap();
    use modo::db::ConnExt;
    let result: u64 = db
        .conn()
        .execute_raw("CREATE TABLE test_default (id TEXT PRIMARY KEY)", ())
        .await
        .unwrap();
    assert_eq!(result, 0);
}

#[tokio::test]
async fn pool_conn_shard_opens_new_database() {
    let dir = tempfile::tempdir().unwrap();
    let config = db::Config {
        path: ":memory:".to_string(),
        pool: Some(db::PoolConfig {
            base_path: dir.path().to_str().unwrap().to_string(),
            shard_count: 4,
        }),
        ..Default::default()
    };
    let pool = db::DatabasePool::new(&config).await.unwrap();

    // First call to a shard creates the database
    let shard_db = pool.conn(Some("tenant_abc")).await.unwrap();
    use modo::db::ConnExt;
    shard_db
        .conn()
        .execute_raw("CREATE TABLE shard_test (id TEXT PRIMARY KEY)", ())
        .await
        .unwrap();

    // Second call returns the cached connection — table should exist
    let shard_db2 = pool.conn(Some("tenant_abc")).await.unwrap();
    shard_db2
        .conn()
        .execute_raw("INSERT INTO shard_test (id) VALUES ('hello')", ())
        .await
        .unwrap();
}

#[tokio::test]
async fn pool_shards_are_independent() {
    let dir = tempfile::tempdir().unwrap();
    let config = db::Config {
        path: ":memory:".to_string(),
        pool: Some(db::PoolConfig {
            base_path: dir.path().to_str().unwrap().to_string(),
            shard_count: 4,
        }),
        ..Default::default()
    };
    let pool = db::DatabasePool::new(&config).await.unwrap();

    // Create a table in shard A
    use modo::db::ConnExt;
    let shard_a = pool.conn(Some("shard_a")).await.unwrap();
    shard_a
        .conn()
        .execute_raw("CREATE TABLE only_in_a (id TEXT PRIMARY KEY)", ())
        .await
        .unwrap();

    // Shard B should NOT have that table
    let shard_b = pool.conn(Some("shard_b")).await.unwrap();
    let err = shard_b
        .conn()
        .execute_raw("INSERT INTO only_in_a (id) VALUES ('x')", ())
        .await;
    assert!(err.is_err());
}

#[tokio::test]
async fn pool_is_clone() {
    let config = db::Config {
        path: ":memory:".to_string(),
        pool: Some(db::PoolConfig {
            base_path: "data/test_shards".to_string(),
            shard_count: 4,
        }),
        ..Default::default()
    };
    let pool = db::DatabasePool::new(&config).await.unwrap();
    let pool2 = pool.clone();

    // Both clones access the same default database
    use modo::db::ConnExt;
    pool.conn(None)
        .await
        .unwrap()
        .conn()
        .execute_raw("CREATE TABLE clone_test (id TEXT PRIMARY KEY)", ())
        .await
        .unwrap();

    pool2
        .conn(None)
        .await
        .unwrap()
        .conn()
        .execute_raw("INSERT INTO clone_test (id) VALUES ('from_clone2')", ())
        .await
        .unwrap();
}

#[tokio::test]
async fn pool_shard_runs_migrations() {
    let dir = tempfile::tempdir().unwrap();
    let migrations_dir = dir.path().join("migrations");
    std::fs::create_dir_all(&migrations_dir).unwrap();
    std::fs::write(
        migrations_dir.join("001_create_users.sql"),
        "CREATE TABLE IF NOT EXISTS users (id TEXT PRIMARY KEY, name TEXT NOT NULL);",
    )
    .unwrap();

    let config = db::Config {
        path: dir.path().join("main.db").to_str().unwrap().to_string(),
        migrations: Some(migrations_dir.to_str().unwrap().to_string()),
        pool: Some(db::PoolConfig {
            base_path: dir.path().join("shards").to_str().unwrap().to_string(),
            shard_count: 4,
        }),
        ..Default::default()
    };
    let pool = db::DatabasePool::new(&config).await.unwrap();

    // Shard should have the users table from migrations
    use modo::db::ConnExt;
    let shard = pool.conn(Some("tenant_xyz")).await.unwrap();
    shard
        .conn()
        .execute_raw(
            "INSERT INTO users (id, name) VALUES ('u1', 'Alice')",
            (),
        )
        .await
        .unwrap();
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --features db --test db_pool_test -- --nocapture`
Expected: FAIL — `DatabasePool` does not exist.

- [ ] **Step 3: Implement `DatabasePool`**

Add to `src/db/pool.rs`, above the `#[cfg(test)]` block:

```rust
use std::sync::Arc;

use crate::error::{Error, Result};

use super::config::{Config, PoolConfig};
use super::connect::connect;

// ---------------------------------------------------------------------------
// DatabasePool
// ---------------------------------------------------------------------------

/// Multi-database connection pool with lazy shard opening.
///
/// Wraps a default [`Database`] (the main database) plus a sharded cache of
/// lazily-opened shard databases. All shards share the same PRAGMAs and
/// migrations from the parent [`Config`].
///
/// Cloning is cheap (reference count increment via `Arc`).
///
/// # Examples
///
/// ```rust,ignore
/// use modo::db::{self, ConnExt, ConnQueryExt, DatabasePool};
///
/// let pool = DatabasePool::new(&config).await?;
///
/// // Default database:
/// let user: User = pool.conn(None).await?
///     .conn()
///     .query_one("SELECT id, name FROM users WHERE id = ?1", libsql::params!["u1"])
///     .await?;
///
/// // Tenant shard (lazy open + cache):
/// let user: User = pool.conn(tenant.db_shard.as_deref()).await?
///     .conn()
///     .query_one("SELECT id, name FROM users WHERE id = ?1", libsql::params!["u1"])
///     .await?;
/// ```
#[derive(Clone)]
pub struct DatabasePool {
    inner: Arc<Inner>,
}

struct Inner {
    default: Database,
    config: Config,
    pool_config: PoolConfig,
    shards: ShardedMap,
}

impl DatabasePool {
    /// Create a new pool from the given config.
    ///
    /// Opens the default database immediately. Shard databases are opened
    /// lazily on first [`conn`](Self::conn) call.
    ///
    /// # Errors
    ///
    /// Returns an error if `config.pool` is `None` or the default database
    /// fails to open.
    pub async fn new(config: &Config) -> Result<Self> {
        let pool_config = config
            .pool
            .clone()
            .ok_or_else(|| Error::internal("database pool config is required"))?;

        let default = connect(config).await?;
        let shards = ShardedMap::new(pool_config.shard_count);

        Ok(Self {
            inner: Arc::new(Inner {
                default,
                config: config.clone(),
                pool_config,
                shards,
            }),
        })
    }

    /// Get a database connection by shard name.
    ///
    /// - `None` — returns the default database (instant, no lock).
    /// - `Some("name")` — returns the cached shard database, opening it on
    ///   first access at `{base_path}/{name}.db`.
    pub async fn conn(&self, shard: Option<&str>) -> Result<Database> {
        let Some(name) = shard else {
            return Ok(self.inner.default.clone());
        };

        // Fast path: read-lock lookup
        if let Some(db) = self.inner.shards.get(name) {
            return Ok(db);
        }

        // Slow path: open new connection, then insert
        let shard_path = if self.inner.pool_config.base_path == ":memory:" {
            ":memory:".to_string()
        } else {
            format!("{}/{}.db", self.inner.pool_config.base_path, name)
        };
        let shard_config = Config {
            path: shard_path,
            pool: None, // shards don't nest pools
            ..self.inner.config.clone()
        };

        let db = connect(&shard_config).await.map_err(|e| {
            Error::internal(format!("failed to open shard database: {name}")).chain(e)
        })?;

        self.inner.shards.insert(name.to_string(), db.clone());
        Ok(db)
    }
}
```

- [ ] **Step 4: Add public re-exports to `src/db/mod.rs`**

Change the pool module declaration from:

```rust
mod pool;
```

to:

```rust
mod pool;
pub use pool::DatabasePool;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --features db --test db_pool_test -- --nocapture`
Expected: All tests PASS.

- [ ] **Step 6: Run clippy**

Run: `cargo clippy --features db --tests -- -D warnings`
Expected: No warnings.

- [ ] **Step 7: Commit**

```
git add src/db/pool.rs src/db/mod.rs tests/db_pool_test.rs
git commit -m "feat(db): implement DatabasePool with lazy shard opening"
```

---

### Task 4: Add `ManagedDatabasePool` and `managed_pool`

**Files:**
- Modify: `src/db/pool.rs`
- Modify: `src/db/mod.rs`
- Modify: `tests/db_pool_test.rs`

- [ ] **Step 1: Write the failing test**

Append to `tests/db_pool_test.rs`:

```rust
#[tokio::test]
async fn managed_pool_can_shutdown() {
    let config = db::Config {
        path: ":memory:".to_string(),
        pool: Some(db::PoolConfig {
            base_path: "data/test_shards".to_string(),
            shard_count: 4,
        }),
        ..Default::default()
    };
    let pool = db::DatabasePool::new(&config).await.unwrap();
    let managed = db::managed_pool(pool);
    // Verify it implements Task by calling shutdown
    use modo::runtime::Task;
    managed.shutdown().await.unwrap();
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --features db --test db_pool_test managed_pool -- --nocapture`
Expected: FAIL — `managed_pool` does not exist.

- [ ] **Step 3: Add `ManagedDatabasePool` and `managed_pool`**

Add to `src/db/pool.rs`, after the `DatabasePool` impl block:

```rust
// ---------------------------------------------------------------------------
// Graceful shutdown
// ---------------------------------------------------------------------------

/// Wrapper for graceful shutdown integration with [`crate::run!`].
///
/// Wraps a [`DatabasePool`] so it can be registered as a [`Task`](crate::runtime::Task)
/// with the modo runtime. On shutdown all database handles (default and shards)
/// are dropped.
///
/// Created by [`managed_pool`].
pub struct ManagedDatabasePool(DatabasePool);

impl crate::runtime::Task for ManagedDatabasePool {
    async fn shutdown(self) -> Result<()> {
        drop(self.0);
        Ok(())
    }
}

/// Wrap a [`DatabasePool`] for use with [`crate::run!`].
///
/// # Examples
///
/// ```rust,no_run
/// use modo::db;
///
/// # async fn example() -> modo::Result<()> {
/// let config = db::Config::default();
/// let pool = db::DatabasePool::new(&config).await?;
/// let task = db::managed_pool(pool.clone());
/// // Register `task` with modo::run!() for graceful shutdown
/// # Ok(())
/// # }
/// ```
pub fn managed_pool(pool: DatabasePool) -> ManagedDatabasePool {
    ManagedDatabasePool(pool)
}
```

- [ ] **Step 4: Update re-exports in `src/db/mod.rs`**

Change the pool re-export line from:

```rust
pub use pool::DatabasePool;
```

to:

```rust
pub use pool::{DatabasePool, ManagedDatabasePool, managed_pool};
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --features db --test db_pool_test -- --nocapture`
Expected: All tests PASS.

- [ ] **Step 6: Commit**

```
git add src/db/pool.rs src/db/mod.rs tests/db_pool_test.rs
git commit -m "feat(db): add ManagedDatabasePool for graceful shutdown"
```

---

### Task 5: Add `TestPool`

**Files:**
- Create: `src/testing/pool.rs`
- Modify: `src/testing/mod.rs`
- Create: `tests/testing_pool_test.rs`

- [ ] **Step 1: Write the failing integration test**

Create `tests/testing_pool_test.rs`:

```rust
#![cfg(feature = "test-helpers")]

use modo::db::{ConnExt, ConnQueryExt, FromRow};
use modo::testing::TestPool;

#[derive(Debug)]
struct Item {
    id: String,
    name: String,
}

impl FromRow for Item {
    fn from_row(row: &libsql::Row) -> modo::Result<Self> {
        Ok(Self {
            id: row.get::<String>(0).map_err(modo::Error::from)?,
            name: row.get::<String>(1).map_err(modo::Error::from)?,
        })
    }
}

#[tokio::test]
async fn test_pool_default_works() {
    let pool = TestPool::new().await;
    let db = pool.conn(None).await.unwrap();
    db.conn()
        .execute_raw(
            "CREATE TABLE items (id TEXT PRIMARY KEY, name TEXT NOT NULL)",
            (),
        )
        .await
        .unwrap();
    db.conn()
        .execute_raw(
            "INSERT INTO items (id, name) VALUES ('i1', 'Widget')",
            (),
        )
        .await
        .unwrap();
    let item: Item = db
        .conn()
        .query_one("SELECT id, name FROM items WHERE id = ?1", libsql::params!["i1"])
        .await
        .unwrap();
    assert_eq!(item.id, "i1");
    assert_eq!(item.name, "Widget");
}

#[tokio::test]
async fn test_pool_shard_is_independent() {
    let pool = TestPool::new().await;

    // Create table in default
    pool.conn(None)
        .await
        .unwrap()
        .conn()
        .execute_raw("CREATE TABLE t (id TEXT PRIMARY KEY)", ())
        .await
        .unwrap();

    // Shard should NOT have it (independent in-memory DB)
    let shard = pool.conn(Some("tenant_a")).await.unwrap();
    let err = shard
        .conn()
        .execute_raw("INSERT INTO t (id) VALUES ('x')", ())
        .await;
    assert!(err.is_err());
}

#[tokio::test]
async fn test_pool_shard_is_cached() {
    let pool = TestPool::new().await;

    // First access creates, second reuses
    let shard1 = pool.conn(Some("tenant_b")).await.unwrap();
    shard1
        .conn()
        .execute_raw("CREATE TABLE cached (id TEXT PRIMARY KEY)", ())
        .await
        .unwrap();

    let shard2 = pool.conn(Some("tenant_b")).await.unwrap();
    shard2
        .conn()
        .execute_raw("INSERT INTO cached (id) VALUES ('yes')", ())
        .await
        .unwrap();
}

#[tokio::test]
async fn test_pool_exec_chaining() {
    let pool = TestPool::new()
        .await
        .exec(None, "CREATE TABLE chained (id TEXT PRIMARY KEY)")
        .await
        .exec(
            Some("shard_x"),
            "CREATE TABLE chained (id TEXT PRIMARY KEY)",
        )
        .await;

    pool.conn(None)
        .await
        .unwrap()
        .conn()
        .execute_raw("INSERT INTO chained (id) VALUES ('a')", ())
        .await
        .unwrap();

    pool.conn(Some("shard_x"))
        .await
        .unwrap()
        .conn()
        .execute_raw("INSERT INTO chained (id) VALUES ('b')", ())
        .await
        .unwrap();
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --features test-helpers --test testing_pool_test -- --nocapture`
Expected: FAIL — `TestPool` does not exist.

- [ ] **Step 3: Implement `TestPool`**

Create `src/testing/pool.rs`:

```rust
use crate::db::{Config, Database, DatabasePool, PoolConfig, connect};
use crate::error::Result;

/// An in-memory database pool for use in tests.
///
/// Both the default database and all shards use `:memory:` — no file I/O.
/// The builder-style [`exec`](TestPool::exec) method returns `Self` for
/// chaining.
///
/// ```rust,no_run
/// # #[cfg(feature = "test-helpers")]
/// # async fn example() {
/// use modo::testing::TestPool;
///
/// let pool = TestPool::new()
///     .await
///     .exec(None, "CREATE TABLE items (id TEXT PRIMARY KEY, name TEXT NOT NULL)")
///     .await;
///
/// let db = pool.conn(None).await.unwrap();
/// # }
/// ```
pub struct TestPool {
    pool: DatabasePool,
}

impl TestPool {
    /// Create a new in-memory database pool.
    ///
    /// # Panics
    ///
    /// Panics if the pool cannot be created.
    pub async fn new() -> Self {
        let config = Config {
            path: ":memory:".to_string(),
            pool: Some(PoolConfig {
                base_path: ":memory:".to_string(),
                shard_count: 4,
            }),
            ..Default::default()
        };
        let pool = DatabasePool::new(&config)
            .await
            .expect("failed to create test pool");
        Self { pool }
    }

    /// Execute a raw SQL statement on the given shard (or default) and return
    /// `self` for chaining.
    ///
    /// # Panics
    ///
    /// Panics if the statement fails.
    pub async fn exec(self, shard: Option<&str>, sql: &str) -> Self {
        use crate::db::ConnExt;
        let db = self
            .pool
            .conn(shard)
            .await
            .unwrap_or_else(|e| panic!("failed to get connection for shard {shard:?}: {e}"));
        db.conn()
            .execute_raw(sql, ())
            .await
            .unwrap_or_else(|e| panic!("failed to execute SQL: {e}\nSQL: {sql}"));
        self
    }

    /// Get a database connection by shard name.
    ///
    /// See [`DatabasePool::conn`] for details.
    pub async fn conn(&self, shard: Option<&str>) -> Result<Database> {
        self.pool.conn(shard).await
    }

    /// Return a cloned [`DatabasePool`] handle.
    pub fn pool(&self) -> DatabasePool {
        self.pool.clone()
    }
}
```

- [ ] **Step 4: Register module and re-export in `src/testing/mod.rs`**

Add module declaration alongside the existing ones:

```rust
mod pool;
```

Add to the public re-exports:

```rust
pub use pool::TestPool;
```

- [ ] **Step 5: Handle in-memory shards in `DatabasePool::conn`**

The current implementation builds shard paths as `{base_path}/{name}.db`. When `base_path` is `:memory:`, we need to use `:memory:` directly (not `:memory:/name.db`). This was already handled in Task 3 Step 3 with the `:memory:` check — verify it is present:

```rust
        let shard_path = if self.inner.pool_config.base_path == ":memory:" {
            ":memory:".to_string()
        } else {
            format!("{}/{}.db", self.inner.pool_config.base_path, name)
        };
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test --features test-helpers --test testing_pool_test -- --nocapture`
Expected: All 4 tests PASS.

- [ ] **Step 7: Run all tests to verify no regressions**

Run: `cargo test --features test-helpers`
Expected: All tests PASS.

- [ ] **Step 8: Run clippy**

Run: `cargo clippy --features test-helpers --tests -- -D warnings`
Expected: No warnings.

- [ ] **Step 9: Commit**

```
git add src/testing/pool.rs src/testing/mod.rs tests/testing_pool_test.rs
git commit -m "feat(testing): add TestPool for in-memory shard testing"
```

---

### Task 6: Final polish and full test run

**Files:**
- All files from previous tasks

- [ ] **Step 1: Run the full test suite**

Run: `cargo test --features test-helpers`
Expected: All tests PASS.

- [ ] **Step 2: Run clippy on all feature combinations**

Run: `cargo clippy --features db --tests -- -D warnings`
Run: `cargo clippy --features test-helpers --tests -- -D warnings`
Expected: No warnings.

- [ ] **Step 3: Run fmt check**

Run: `cargo fmt --check`
Expected: No formatting issues.

- [ ] **Step 4: Commit any fixes if needed**

Only commit if there are changes from the previous steps. Skip if clean.
