use std::path::Path;

use crate::error::{Error, Result};

use super::pool::Writer;

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
