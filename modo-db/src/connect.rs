use crate::config::DatabaseConfig;
use crate::pool::DbPool;
use sea_orm::{ConnectOptions, Database};
use std::time::Duration;
use tracing::info;

/// Connect to the database using the provided configuration.
///
/// Auto-detects the backend from the URL scheme and applies
/// backend-specific settings (SQLite pragmas, Postgres pool tuning).
pub async fn connect(config: &DatabaseConfig) -> Result<DbPool, modo::Error> {
    let mut opts = ConnectOptions::new(&config.url);
    opts.max_connections(config.max_connections)
        .min_connections(config.min_connections)
        .acquire_timeout(Duration::from_secs(config.acquire_timeout_secs))
        .idle_timeout(Duration::from_secs(config.idle_timeout_secs))
        .max_lifetime(Duration::from_secs(config.max_lifetime_secs));

    let conn = Database::connect(opts)
        .await
        .map_err(|e| modo::Error::internal(format!("database connection failed: {e}")))?;

    // Apply backend-specific settings
    if conn.get_database_backend() == sea_orm::DatabaseBackend::Sqlite {
        apply_sqlite_pragmas(&conn).await?;
    }

    info!(url = %redact_url(&config.url), "Database connected");
    Ok(DbPool(conn))
}

#[cfg(feature = "sqlite")]
async fn apply_sqlite_pragmas(conn: &sea_orm::DatabaseConnection) -> Result<(), modo::Error> {
    use sea_orm::ConnectionTrait;

    conn.execute_unprepared("PRAGMA journal_mode=WAL")
        .await
        .map_err(|e| modo::Error::internal(format!("failed to set WAL mode: {e}")))?;
    conn.execute_unprepared("PRAGMA busy_timeout=5000")
        .await
        .map_err(|e| modo::Error::internal(format!("failed to set busy_timeout: {e}")))?;
    conn.execute_unprepared("PRAGMA synchronous=NORMAL")
        .await
        .map_err(|e| modo::Error::internal(format!("failed to set synchronous: {e}")))?;
    conn.execute_unprepared("PRAGMA foreign_keys=ON")
        .await
        .map_err(|e| modo::Error::internal(format!("failed to enable foreign_keys: {e}")))?;
    Ok(())
}

#[cfg(not(feature = "sqlite"))]
async fn apply_sqlite_pragmas(_conn: &sea_orm::DatabaseConnection) -> Result<(), modo::Error> {
    Err(modo::Error::internal(
        "SQLite URL provided but `sqlite` feature is not enabled",
    ))
}

/// Redact credentials from database URL for logging.
fn redact_url(url: &str) -> String {
    if let Some(scheme_end) = url.find("://") {
        let authority_start = scheme_end + 3;
        if let Some(relative_at) = url[authority_start..].find('@') {
            let prefix = &url[..authority_start];
            let suffix = &url[authority_start + relative_at..];
            return format!("{prefix}***{suffix}");
        }
    }
    url.to_string()
}
