use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::Path;
use std::sync::{Arc, RwLock};

use crate::error::{Error, Result};

use super::config::Config;
use super::connect::connect;
use super::database::Database;

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
            .as_ref()
            .ok_or_else(|| Error::internal("database pool config is required"))?;

        if pool_config.lock_shards == 0 {
            return Err(Error::internal("pool lock_shards must be greater than 0"));
        }

        let default = connect(config).await?;
        let shards = ShardedMap::new(pool_config.lock_shards);

        Ok(Self {
            inner: Arc::new(Inner {
                default,
                config: config.clone(),
                shards,
            }),
        })
    }

    /// Get a database connection by shard name.
    ///
    /// - `None` — returns the default database (instant, no lock).
    /// - `Some("name")` — returns the cached shard database, opening it on
    ///   first access at `{base_path}/{name}.db`.
    ///
    /// Concurrent first-access to the same shard may open duplicate
    /// connections; the last writer wins and the extra connection is dropped.
    /// This is benign because `connect` is idempotent (PRAGMAs are
    /// re-applied, migrations use checksum tracking).
    ///
    /// # Errors
    ///
    /// Returns an error if the shard name is invalid (empty, starts with `.`,
    /// or contains path separators) or if the shard database fails to open.
    pub async fn conn(&self, shard: Option<&str>) -> Result<Database> {
        let Some(name) = shard else {
            return Ok(self.inner.default.clone());
        };

        if name.is_empty()
            || name.starts_with('.')
            || name.contains('/')
            || name.contains('\\')
            || name.contains('\0')
        {
            return Err(Error::bad_request(format!("invalid shard name: {name:?}")));
        }

        if let Some(db) = self.inner.shards.get(name) {
            return Ok(db);
        }

        // Safety: pool config is validated as Some in new()
        let pool_config = self.inner.config.pool.as_ref().unwrap();
        let shard_path = if pool_config.base_path == ":memory:" {
            ":memory:".to_string()
        } else {
            Path::new(&pool_config.base_path)
                .join(format!("{name}.db"))
                .to_string_lossy()
                .into_owned()
        };
        let shard_config = Config {
            path: shard_path,
            pool: None,
            ..self.inner.config.clone()
        };

        let db = connect(&shard_config).await.map_err(|e| {
            Error::internal(format!("failed to open shard database: {name}")).chain(e)
        })?;

        self.inner.shards.insert(name.to_string(), db.clone());
        Ok(db)
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    async fn make_test_db() -> Database {
        let config = super::super::config::Config {
            path: ":memory:".to_string(),
            ..Default::default()
        };
        super::super::connect::connect(&config).await.unwrap()
    }

    #[test]
    fn sharded_map_get_returns_none_for_missing_key() {
        let map = ShardedMap::new(4);
        assert!(map.get("missing").is_none());
    }

    #[tokio::test]
    async fn sharded_map_insert_and_get() {
        let map = ShardedMap::new(4);
        let db = make_test_db().await;
        map.insert("tenant_a".to_string(), db);
        assert!(map.get("tenant_a").is_some());
    }

    #[tokio::test]
    async fn sharded_map_different_keys_independent() {
        let map = ShardedMap::new(4);
        let db = make_test_db().await;
        map.insert("tenant_a".to_string(), db);
        assert!(map.get("tenant_a").is_some());
        assert!(map.get("tenant_b").is_none());
    }

    #[tokio::test]
    async fn sharded_map_insert_idempotent() {
        let map = ShardedMap::new(4);
        let db1 = make_test_db().await;
        let db2 = make_test_db().await;
        map.insert("key".to_string(), db1);
        map.insert("key".to_string(), db2);
        assert!(map.get("key").is_some());
    }
}
