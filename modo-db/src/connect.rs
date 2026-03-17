use crate::config::DatabaseConfig;
use crate::pool::DbPool;
use std::time::Duration;
use tracing::info;

/// Connect to the database using the provided configuration.
///
/// Backend is selected from the config:
/// - `sqlite` set → SQLite (builds sqlx pool with per-connection PRAGMAs)
/// - `postgres` set → Postgres (uses SeaORM ConnectOptions)
/// - Neither set → defaults to SQLite with `SqliteDbConfig::default()`
/// - Both set → returns an error
pub async fn connect(config: &DatabaseConfig) -> Result<DbPool, modo::Error> {
    match (&config.sqlite, &config.postgres) {
        (Some(_), Some(_)) => {
            return Err(modo::Error::internal(
                "both `sqlite` and `postgres` are set in database config — pick one",
            ));
        }
        (Some(sqlite), None) => connect_sqlite(sqlite, config).await,
        (None, Some(pg)) => connect_postgres(pg, config).await,
        (None, None) => {
            // Default to SQLite when neither is configured
            let sqlite = crate::config::SqliteDbConfig::default();
            connect_sqlite(&sqlite, config).await
        }
    }
}

#[cfg(feature = "sqlite")]
async fn connect_sqlite(
    sqlite: &crate::config::SqliteDbConfig,
    config: &DatabaseConfig,
) -> Result<DbPool, modo::Error> {
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use std::str::FromStr;

    // Build the connection URL
    let url = if sqlite.path == ":memory:" {
        "sqlite::memory:".to_string()
    } else {
        // Ensure parent directory exists
        if let Some(parent) = std::path::Path::new(&sqlite.path).parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    modo::Error::internal(format!(
                        "failed to create database directory {}: {e}",
                        parent.display()
                    ))
                })?;
            }
        }
        format!("sqlite://{}?mode=rwc", sqlite.path)
    };

    let connect_options = SqliteConnectOptions::from_str(&url)
        .map_err(|e| modo::Error::internal(format!("invalid SQLite URL: {e}")))?;

    let pragmas = sqlite.pragmas.clone();
    let pool = SqlitePoolOptions::new()
        .max_connections(config.max_connections)
        .min_connections(config.min_connections)
        .acquire_timeout(Duration::from_secs(config.acquire_timeout_secs))
        .idle_timeout(Duration::from_secs(config.idle_timeout_secs))
        .max_lifetime(Duration::from_secs(config.max_lifetime_secs))
        .after_connect(move |conn, _meta| {
            let pragmas = pragmas.clone();
            Box::pin(async move { apply_sqlite_pragmas(conn, &pragmas).await })
        })
        .connect_with(connect_options)
        .await
        .map_err(|e| modo::Error::internal(format!("SQLite connection failed: {e}")))?;

    let db_conn = sea_orm::SqlxSqliteConnector::from_sqlx_sqlite_pool(pool);
    info!(path = %sqlite.path, "SQLite database connected");
    Ok(DbPool(db_conn))
}

#[cfg(not(feature = "sqlite"))]
async fn connect_sqlite(
    _sqlite: &crate::config::SqliteDbConfig,
    _config: &DatabaseConfig,
) -> Result<DbPool, modo::Error> {
    Err(modo::Error::internal(
        "SQLite config provided but `sqlite` feature is not enabled",
    ))
}

async fn connect_postgres(
    pg: &crate::config::PostgresDbConfig,
    config: &DatabaseConfig,
) -> Result<DbPool, modo::Error> {
    use sea_orm::{ConnectOptions, Database};

    let mut opts = ConnectOptions::new(&pg.url);
    opts.max_connections(config.max_connections)
        .min_connections(config.min_connections)
        .acquire_timeout(Duration::from_secs(config.acquire_timeout_secs))
        .idle_timeout(Duration::from_secs(config.idle_timeout_secs))
        .max_lifetime(Duration::from_secs(config.max_lifetime_secs));

    let conn = Database::connect(opts)
        .await
        .map_err(|e| modo::Error::internal(format!("Postgres connection failed: {e}")))?;

    info!(url = %redact_url(&pg.url), "Postgres database connected");
    Ok(DbPool(conn))
}

#[cfg(feature = "sqlite")]
async fn apply_sqlite_pragmas(
    conn: &mut sqlx::sqlite::SqliteConnection,
    config: &crate::config::SqliteConfig,
) -> Result<(), sqlx::Error> {
    use sqlx::Executor;

    conn.execute(format!("PRAGMA journal_mode={}", config.journal_mode).as_str())
        .await?;
    conn.execute(format!("PRAGMA busy_timeout={}", config.busy_timeout).as_str())
        .await?;
    conn.execute(format!("PRAGMA synchronous={}", config.synchronous).as_str())
        .await?;
    conn.execute(
        format!(
            "PRAGMA foreign_keys={}",
            if config.foreign_keys { "ON" } else { "OFF" }
        )
        .as_str(),
    )
    .await?;
    conn.execute(format!("PRAGMA cache_size={}", config.cache_size).as_str())
        .await?;

    if let Some(mmap_size) = config.mmap_size {
        conn.execute(format!("PRAGMA mmap_size={mmap_size}").as_str())
            .await?;
    }
    if let Some(ref temp_store) = config.temp_store {
        conn.execute(format!("PRAGMA temp_store={temp_store}").as_str())
            .await?;
    }
    if let Some(wal_autocheckpoint) = config.wal_autocheckpoint {
        conn.execute(format!("PRAGMA wal_autocheckpoint={wal_autocheckpoint}").as_str())
            .await?;
    }

    Ok(())
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
