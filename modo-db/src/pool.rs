use sea_orm::DatabaseConnection;
use std::future::Future;
use std::ops::Deref;
use std::pin::Pin;

/// Newtype around `sea_orm::DatabaseConnection`.
///
/// Registered as a managed service via `app.managed_service(db)` (which also
/// handles graceful pool shutdown) and extracted in handlers via the [`Db`]
/// extractor.
///
/// [`Db`]: crate::extractor::Db
#[derive(Debug, Clone)]
pub struct DbPool(pub(crate) DatabaseConnection);

impl DbPool {
    /// Access the underlying SeaORM connection.
    pub fn connection(&self) -> &DatabaseConnection {
        &self.0
    }
}

impl Deref for DbPool {
    type Target = DatabaseConnection;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl modo::GracefulShutdown for DbPool {
    fn graceful_shutdown(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(async {
            if let Err(e) = self.0.close_by_ref().await {
                tracing::warn!("Error closing database pool: {e}");
            }
        })
    }
}
