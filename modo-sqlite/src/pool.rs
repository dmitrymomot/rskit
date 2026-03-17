use std::future::Future;
use std::pin::Pin;

/// Marker trait for pool types that can be used for database writes and migrations.
///
/// Both [`Pool`] and [`WritePool`] implement this trait. [`ReadPool`] intentionally
/// does **not**, so that write-path operations (including migrations) can only be
/// called with a writable pool at compile time.
///
/// # Compile-time enforcement
///
/// The following snippet must fail to compile because `ReadPool` does not implement `AsPool`:
///
/// ```compile_fail
/// # use modo_sqlite::pool::{AsPool, ReadPool};
/// fn _assert(_: &impl AsPool) {}
/// _assert(&ReadPool(todo!()));
/// ```
pub trait AsPool {
    /// Returns a reference to the underlying [`sqlx::SqlitePool`].
    fn pool(&self) -> &sqlx::SqlitePool;
}

/// A general-purpose SQLite connection pool (readable and writable).
///
/// Implements [`AsPool`] and [`modo::GracefulShutdown`].
/// Obtained from [`crate::connect()`].
#[derive(Debug, Clone)]
pub struct Pool(pub(crate) sqlx::SqlitePool);

/// A read-only SQLite connection pool.
///
/// Does **not** implement [`AsPool`] — write-path operations (e.g. migrations)
/// are intentionally unavailable through this type.
/// Obtained from [`crate::connect_rw`].
#[derive(Debug, Clone)]
pub struct ReadPool(pub(crate) sqlx::SqlitePool);

/// A write-only SQLite connection pool.
///
/// Implements [`AsPool`] and [`modo::GracefulShutdown`].
/// Obtained from [`crate::connect_rw`].
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
