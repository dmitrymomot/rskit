use std::ops::Deref;

/// The underlying sqlx SQLite connection pool type.
///
/// Re-export of [`sqlx::SqlitePool`] used as the inner storage for the pool
/// newtypes. Prefer [`Pool`], [`ReadPool`], or [`WritePool`] in application
/// code.
pub type InnerPool = sqlx::SqlitePool;

/// A general-purpose SQLite connection pool for both reads and writes.
///
/// Implements both [`Reader`] and [`Writer`], so it can be used for any query.
/// For in-memory databases (`":memory:"`), use this type exclusively — see
/// [`connect`](super::connect::connect).
///
/// Derefs to [`InnerPool`] for direct use with sqlx queries.
#[derive(Clone)]
pub struct Pool(InnerPool);

/// A read-only SQLite connection pool.
///
/// Implements [`Reader`] but not [`Writer`], preventing accidental writes
/// through this handle. Created by
/// [`connect_rw`](super::connect::connect_rw).
///
/// Derefs to [`InnerPool`] for direct use with sqlx queries.
#[derive(Clone)]
pub struct ReadPool(InnerPool);

/// A write-capable SQLite connection pool.
///
/// Implements both [`Reader`] and [`Writer`]. Created by
/// [`connect_rw`](super::connect::connect_rw) and intentionally defaults to
/// `max_connections = 1` to serialize writes.
///
/// Derefs to [`InnerPool`] for direct use with sqlx queries.
#[derive(Clone)]
pub struct WritePool(InnerPool);

/// Provides read access to the underlying [`InnerPool`].
///
/// Implemented by [`Pool`], [`ReadPool`], and [`WritePool`]. Pass
/// `&impl Reader` to functions that only need to execute SELECT queries.
pub trait Reader {
    /// Returns a reference to the underlying read pool.
    fn read_pool(&self) -> &InnerPool;
}

/// Provides write access to the underlying [`InnerPool`].
///
/// Implemented by [`Pool`] and [`WritePool`]. Required by
/// [`migrate`](super::migrate::migrate) and any function that mutates the
/// database. [`ReadPool`] intentionally does not implement this trait.
pub trait Writer {
    /// Returns a reference to the underlying write pool.
    fn write_pool(&self) -> &InnerPool;
}

impl Pool {
    /// Creates a `Pool` from an [`InnerPool`].
    pub fn new(pool: InnerPool) -> Self {
        Self(pool)
    }

    pub(crate) fn into_inner(self) -> InnerPool {
        self.0
    }
}

impl ReadPool {
    /// Creates a `ReadPool` from an [`InnerPool`].
    ///
    /// Use this to share an existing in-memory pool as a read pool without
    /// creating a second connection (in-memory databases cannot be split):
    ///
    /// ```ignore
    /// let pool = db::connect(&config).await?;
    /// let read_pool = db::ReadPool::new((*pool).clone());
    /// let write_pool = db::WritePool::new((*pool).clone());
    /// ```
    pub fn new(pool: InnerPool) -> Self {
        Self(pool)
    }

    pub(crate) fn into_inner(self) -> InnerPool {
        self.0
    }
}

impl WritePool {
    /// Creates a `WritePool` from an [`InnerPool`].
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
