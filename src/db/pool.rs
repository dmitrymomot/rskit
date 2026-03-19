use std::ops::Deref;

#[cfg(feature = "sqlite")]
pub type InnerPool = sqlx::SqlitePool;

#[cfg(feature = "postgres")]
pub type InnerPool = sqlx::PgPool;

#[derive(Clone)]
pub struct Pool(InnerPool);

#[derive(Clone)]
pub struct ReadPool(InnerPool);

#[derive(Clone)]
pub struct WritePool(InnerPool);

pub trait AsPool {
    fn pool(&self) -> &InnerPool;
}

impl Pool {
    pub fn new(pool: InnerPool) -> Self {
        Self(pool)
    }
}

impl ReadPool {
    pub fn new(pool: InnerPool) -> Self {
        Self(pool)
    }
}

impl WritePool {
    pub fn new(pool: InnerPool) -> Self {
        Self(pool)
    }
}

impl AsPool for Pool {
    fn pool(&self) -> &InnerPool {
        &self.0
    }
}

impl AsPool for WritePool {
    fn pool(&self) -> &InnerPool {
        &self.0
    }
}

// ReadPool intentionally does NOT implement AsPool
// to prevent passing it to migration functions.

impl Deref for Pool {
    type Target = InnerPool;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Deref for ReadPool {
    type Target = InnerPool;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Deref for WritePool {
    type Target = InnerPool;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
