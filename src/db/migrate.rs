use std::path::Path;

use crate::error::{Error, Result};

use super::pool::Writer;

/// Runs all pending sqlx migrations from the given directory path.
///
/// Migrations are discovered by [`sqlx::migrate::Migrator`] using sqlx's
/// standard filename convention (e.g. `001_create_users.sql`). They are run
/// in order against the pool returned by [`Writer::write_pool`].
///
/// Accepts any pool that implements [`Writer`]: [`Pool`](super::Pool) or
/// [`WritePool`](super::WritePool).
///
/// # Errors
///
/// Returns [`crate::Error::internal`] if the migration directory cannot be
/// loaded or if any migration fails to apply.
pub async fn migrate(path: &str, pool: &impl Writer) -> Result<()> {
    let migrator = sqlx::migrate::Migrator::new(Path::new(path))
        .await
        .map_err(|e| Error::internal(format!("failed to load migrations from '{path}': {e}")))?;

    migrator
        .run(pool.write_pool())
        .await
        .map_err(|e| Error::internal(format!("failed to run migrations: {e}")))?;

    Ok(())
}
