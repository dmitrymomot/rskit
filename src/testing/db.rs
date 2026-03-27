use crate::db::{Pool, ReadPool, SqliteConfig, WritePool, connect};

/// An in-memory SQLite database for use in tests.
///
/// `TestDb` opens a single `:memory:` SQLite connection and exposes it as
/// [`Pool`], [`ReadPool`], and [`WritePool`] newtypes that all share the
/// **same** underlying connection, which is necessary because SQLite
/// in-memory databases are connection-scoped.
///
/// The builder-style [`exec`](TestDb::exec) and [`migrate`](TestDb::migrate)
/// methods return `Self` so that setup can be chained:
///
/// ```rust,no_run
/// # #[cfg(feature = "test-helpers")]
/// # async fn example() {
/// use modo::testing::TestDb;
///
/// let db = TestDb::new()
///     .await
///     .exec("CREATE TABLE items (id TEXT PRIMARY KEY, name TEXT NOT NULL)")
///     .await;
///
/// let pool = db.pool();
/// # }
/// ```
pub struct TestDb {
    pool: Pool,
}

impl TestDb {
    /// Create a new in-memory SQLite database.
    ///
    /// Panics if the database cannot be opened.
    pub async fn new() -> Self {
        let config = SqliteConfig {
            path: ":memory:".to_string(),
            ..Default::default()
        };
        let pool = connect(&config)
            .await
            .expect("failed to create in-memory database");
        Self { pool }
    }

    /// Execute a raw SQL statement and return `self` for chaining.
    ///
    /// Panics if the statement fails.
    pub async fn exec(self, sql: &str) -> Self {
        sqlx::query(sql)
            .execute(&*self.pool)
            .await
            .unwrap_or_else(|e| panic!("failed to execute SQL: {e}\nSQL: {sql}"));
        self
    }

    /// Run all migrations found under `path` (a directory of `.sql` files)
    /// and return `self` for chaining.
    ///
    /// Panics if the migration path is invalid or any migration fails.
    pub async fn migrate(self, path: &str) -> Self {
        crate::db::migrate(path, &self.pool)
            .await
            .unwrap_or_else(|e| panic!("failed to run migrations from '{path}': {e}"));
        self
    }

    /// Return a cloned [`Pool`] backed by the in-memory database.
    pub fn pool(&self) -> Pool {
        self.pool.clone()
    }

    /// Return a [`ReadPool`] that shares the same in-memory connection.
    pub fn read_pool(&self) -> ReadPool {
        ReadPool::new((*self.pool).clone())
    }

    /// Return a [`WritePool`] that shares the same in-memory connection.
    pub fn write_pool(&self) -> WritePool {
        WritePool::new((*self.pool).clone())
    }
}
