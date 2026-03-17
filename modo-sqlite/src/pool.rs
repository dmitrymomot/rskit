use std::future::Future;
use std::pin::Pin;

/// Trait implemented by pool types that can be used for database writes
/// (and therefore for running migrations).
///
/// # Design rationale
///
/// [`ReadPool`] intentionally does NOT implement `AsPool`. This ensures
/// migrations — and any other write-path code that accepts `&impl AsPool` —
/// can only be called with a writable pool (`Pool` or `WritePool`).
///
/// To verify the compile-time enforcement, the following snippet must fail:
/// ```compile_fail
/// # use modo_sqlite::pool::{AsPool, ReadPool};
/// fn _assert(_: &impl AsPool) {}
/// _assert(&ReadPool(todo!()));
/// ```
pub trait AsPool {
    fn pool(&self) -> &sqlx::SqlitePool;
}

// ReadPool intentionally does NOT implement AsPool.
// This ensures migrations can only run through writable pools.
// To verify: `fn _assert(_: &impl AsPool) {} _assert(&ReadPool(...))` would fail to compile.

/// A general-purpose SQLite connection pool (readable and writable).
#[derive(Debug, Clone)]
pub struct Pool(pub(crate) sqlx::SqlitePool);

/// A read-only SQLite connection pool.
///
/// Does **not** implement [`AsPool`] — write-path operations (e.g. migrations)
/// are intentionally unavailable through this type.
#[derive(Debug, Clone)]
pub struct ReadPool(pub(crate) sqlx::SqlitePool);

/// A write-only SQLite connection pool.
#[derive(Debug, Clone)]
pub struct WritePool(pub(crate) sqlx::SqlitePool);

impl Pool {
    /// Returns a reference to the underlying [`sqlx::SqlitePool`].
    pub fn pool(&self) -> &sqlx::SqlitePool {
        &self.0
    }
}

impl ReadPool {
    /// Returns a reference to the underlying [`sqlx::SqlitePool`].
    pub fn pool(&self) -> &sqlx::SqlitePool {
        &self.0
    }
}

impl WritePool {
    /// Returns a reference to the underlying [`sqlx::SqlitePool`].
    pub fn pool(&self) -> &sqlx::SqlitePool {
        &self.0
    }
}

impl AsPool for Pool {
    fn pool(&self) -> &sqlx::SqlitePool {
        &self.0
    }
}

impl AsPool for WritePool {
    fn pool(&self) -> &sqlx::SqlitePool {
        &self.0
    }
}

impl modo::GracefulShutdown for Pool {
    fn graceful_shutdown(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(async {
            self.0.close().await;
        })
    }
}

impl modo::GracefulShutdown for ReadPool {
    fn graceful_shutdown(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(async {
            self.0.close().await;
        })
    }
}

impl modo::GracefulShutdown for WritePool {
    fn graceful_shutdown(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(async {
            self.0.close().await;
        })
    }
}
