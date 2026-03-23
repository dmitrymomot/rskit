use crate::db::{Pool, ReadPool, SqliteConfig, WritePool, connect};

pub struct TestDb {
    pool: Pool,
}

impl TestDb {
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

    pub async fn exec(self, sql: &str) -> Self {
        sqlx::query(sql)
            .execute(&*self.pool)
            .await
            .unwrap_or_else(|e| panic!("failed to execute SQL: {e}\nSQL: {sql}"));
        self
    }

    pub async fn migrate(self, path: &str) -> Self {
        crate::db::migrate(path, &self.pool)
            .await
            .unwrap_or_else(|e| panic!("failed to run migrations from '{path}': {e}"));
        self
    }

    pub fn pool(&self) -> Pool {
        self.pool.clone()
    }

    pub fn read_pool(&self) -> ReadPool {
        ReadPool::new((*self.pool).clone())
    }

    pub fn write_pool(&self) -> WritePool {
        WritePool::new((*self.pool).clone())
    }
}
