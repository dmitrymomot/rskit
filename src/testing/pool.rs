use crate::db::{Config, Database, DatabasePool, PoolConfig};
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
