use super::pool::InnerPool;
use crate::error::Result;
use crate::runtime::Task;

pub struct ManagedPool {
    pool: InnerPool,
}

impl Task for ManagedPool {
    async fn shutdown(self) -> Result<()> {
        self.pool.close().await;
        Ok(())
    }
}

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
