use crate::error::Result;
use crate::runtime::Task;

use super::database::Database;

/// Wrapper for graceful shutdown integration with [`crate::run!`].
///
/// Wraps a [`Database`] so it can be registered as a [`Task`] with the
/// modo runtime. On shutdown the inner `Database` is dropped, which
/// releases the underlying libsql connection and database handle once the
/// last clone is gone.
///
/// Created by [`managed`].
pub struct ManagedDatabase(Database);

impl Task for ManagedDatabase {
    async fn shutdown(self) -> Result<()> {
        // Dropping Database drops the Arc. When the last reference is dropped,
        // Inner is dropped, which drops libsql::Connection and libsql::Database.
        // libsql handles cleanup internally.
        drop(self.0);
        Ok(())
    }
}

/// Wrap a [`Database`] for use with [`crate::run!`].
///
/// # Examples
///
/// ```rust,no_run
/// use modo::db;
///
/// # async fn example() -> modo::Result<()> {
/// let config = db::Config::default();
/// let db = db::connect(&config).await?;
/// let task = db::managed(db.clone());
/// // Register `task` with modo::run!() for graceful shutdown
/// # Ok(())
/// # }
/// ```
pub fn managed(db: Database) -> ManagedDatabase {
    ManagedDatabase(db)
}
