use super::pool::InnerPool;
use crate::error::Result;
use crate::runtime::Task;

/// A pool wrapper that implements [`Task`] for graceful shutdown.
///
/// When [`Task::shutdown`] is called, the underlying connection pool is
/// closed and all connections are drained. Construct via [`managed`].
pub struct ManagedPool {
    pool: InnerPool,
}

impl Task for ManagedPool {
    async fn shutdown(self) -> Result<()> {
        self.pool.close().await;
        Ok(())
    }
}

/// Wraps a pool for graceful shutdown via the [`Task`] trait.
///
/// This consumes the pool. Clone it first if you need continued access after
/// passing it to the shutdown sequence:
///
/// ```ignore
/// let managed = db::managed(pool.clone());
/// run!(server, managed);
/// ```
///
/// Accepts [`Pool`](super::Pool), [`ReadPool`](super::ReadPool), or
/// [`WritePool`](super::WritePool).
pub fn managed<P: Into<ManagedPool>>(pool: P) -> ManagedPool {
    pool.into()
}

impl From<super::Pool> for ManagedPool {
    fn from(pool: super::Pool) -> Self {
        Self {
            pool: pool.into_inner(),
        }
    }
}

impl From<super::ReadPool> for ManagedPool {
    fn from(pool: super::ReadPool) -> Self {
        Self {
            pool: pool.into_inner(),
        }
    }
}

impl From<super::WritePool> for ManagedPool {
    fn from(pool: super::WritePool) -> Self {
        Self {
            pool: pool.into_inner(),
        }
    }
}
