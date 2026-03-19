use std::path::Path;

use crate::error::{Error, Result};

use super::pool::AsPool;

pub async fn migrate(path: &str, pool: &impl AsPool) -> Result<()> {
    let migrator = sqlx::migrate::Migrator::new(Path::new(path))
        .await
        .map_err(|e| Error::internal(format!("failed to load migrations from '{path}': {e}")))?;

    migrator
        .run(pool.pool())
        .await
        .map_err(|e| Error::internal(format!("failed to run migrations: {e}")))?;

    Ok(())
}
