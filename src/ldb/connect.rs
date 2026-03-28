use crate::error::Result;

use super::config::Config;
use super::database::Database;

/// Open a database, apply PRAGMAs, and optionally run migrations.
pub async fn connect(config: &Config) -> Result<Database> {
    // Create parent directories if needed
    if let Some(parent) = std::path::Path::new(&config.path).parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).map_err(|e| {
            crate::error::Error::internal(format!(
                "failed to create database directory: {parent:?}"
            ))
            .chain(e)
        })?;
    }

    let db = libsql::Builder::new_local(&config.path)
        .build()
        .await
        .map_err(crate::error::Error::from)?;

    let conn = db.connect().map_err(crate::error::Error::from)?;

    // Apply PRAGMAs (use query() because PRAGMAs return rows in libsql)
    conn.query(
        &format!("PRAGMA journal_mode={}", config.journal_mode.as_str()),
        (),
    )
    .await
    .map_err(crate::error::Error::from)?;

    conn.query(
        &format!("PRAGMA synchronous={}", config.synchronous.as_str()),
        (),
    )
    .await
    .map_err(crate::error::Error::from)?;

    conn.query(
        &format!("PRAGMA busy_timeout={}", config.busy_timeout),
        (),
    )
    .await
    .map_err(crate::error::Error::from)?;

    conn.query(
        &format!("PRAGMA cache_size=-{}", config.cache_size),
        (),
    )
    .await
    .map_err(crate::error::Error::from)?;

    conn.query(&format!("PRAGMA mmap_size={}", config.mmap_size), ())
        .await
        .map_err(crate::error::Error::from)?;

    conn.query(
        &format!(
            "PRAGMA foreign_keys={}",
            if config.foreign_keys { "ON" } else { "OFF" }
        ),
        (),
    )
    .await
    .map_err(crate::error::Error::from)?;

    conn.query(
        &format!("PRAGMA temp_store={}", config.temp_store.as_str()),
        (),
    )
    .await
    .map_err(crate::error::Error::from)?;

    // Run migrations if configured
    if let Some(ref migrations_dir) = config.migrations {
        super::migrate::migrate(&conn, migrations_dir).await?;
    }

    Ok(Database::new(db, conn))
}
