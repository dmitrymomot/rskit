use crate::db::{Config, Database, connect};

/// An in-memory SQLite database for use in tests.
///
/// `TestDb` opens a single `:memory:` libsql connection and exposes it
/// as a [`Database`] handle. The builder-style [`exec`](TestDb::exec) and
/// [`migrate`](TestDb::migrate) methods return `Self` so that setup can be
/// chained:
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
/// let database = db.db();
/// # }
/// ```
pub struct TestDb {
    db: Database,
}

impl TestDb {
    /// Create a new in-memory SQLite database.
    ///
    /// # Panics
    ///
    /// Panics if the database cannot be opened.
    pub async fn new() -> Self {
        let config = Config {
            path: ":memory:".to_string(),
            ..Default::default()
        };
        let db = connect(&config)
            .await
            .expect("failed to create test database");
        Self { db }
    }

    /// Execute a raw SQL statement and return `self` for chaining.
    ///
    /// # Panics
    ///
    /// Panics if the statement fails.
    pub async fn exec(self, sql: &str) -> Self {
        use crate::db::ConnExt;
        self.db
            .conn()
            .execute_raw(sql, ())
            .await
            .unwrap_or_else(|e| panic!("failed to execute SQL: {e}\nSQL: {sql}"));
        self
    }

    /// Run all migrations found under `path` (a directory of `.sql` files)
    /// and return `self` for chaining.
    ///
    /// # Panics
    ///
    /// Panics if the migration path is invalid or any migration fails.
    pub async fn migrate(self, path: &str) -> Self {
        crate::db::migrate(self.db.conn(), path)
            .await
            .unwrap_or_else(|e| panic!("failed to run migrations from '{path}': {e}"));
        self
    }

    /// Return a cloned [`Database`] handle backed by the in-memory connection.
    pub fn db(&self) -> Database {
        self.db.clone()
    }
}
