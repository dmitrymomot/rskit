use std::ops::Deref;

pub type InnerPool = sqlx::SqlitePool;

#[derive(Clone)]
pub struct Pool(InnerPool);

#[derive(Clone)]
pub struct ReadPool(InnerPool);

#[derive(Clone)]
pub struct WritePool(InnerPool);

pub trait Reader {
    fn read_pool(&self) -> &InnerPool;
}

pub trait Writer {
    fn write_pool(&self) -> &InnerPool;
}

impl Pool {
    pub fn new(pool: InnerPool) -> Self {
        Self(pool)
    }

    pub(crate) fn into_inner(self) -> InnerPool {
        self.0
    }
}

impl ReadPool {
    pub fn new(pool: InnerPool) -> Self {
        Self(pool)
    }

    pub(crate) fn into_inner(self) -> InnerPool {
        self.0
    }
}

impl WritePool {
    pub fn new(pool: InnerPool) -> Self {
        Self(pool)
    }

    pub(crate) fn into_inner(self) -> InnerPool {
        self.0
    }
}

impl Reader for Pool {
    fn read_pool(&self) -> &InnerPool {
        &self.0
    }
}

impl Writer for Pool {
    fn write_pool(&self) -> &InnerPool {
        &self.0
    }
}

impl Reader for ReadPool {
    fn read_pool(&self) -> &InnerPool {
        &self.0
    }
}

// ReadPool intentionally does NOT implement Writer
// to prevent passing it to migration or write functions.

impl Reader for WritePool {
    fn read_pool(&self) -> &InnerPool {
        &self.0
    }
}

impl Writer for WritePool {
    fn write_pool(&self) -> &InnerPool {
        &self.0
    }
}

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
