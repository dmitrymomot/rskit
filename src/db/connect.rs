use std::time::Duration;

use crate::error::{Error, Result};

use super::config::SqliteConfig;
use super::pool::{Pool, ReadPool, WritePool};

#[cfg(feature = "sqlite")]
pub async fn connect(config: &SqliteConfig) -> Result<Pool> {
    let url = build_url(&config.path)?;
    let pool = build_sqlite_pool(&url, config, None).await?;
    Ok(Pool::new(pool))
}

#[cfg(feature = "sqlite")]
pub async fn connect_rw(config: &SqliteConfig) -> Result<(ReadPool, WritePool)> {
    if config.path == ":memory:" {
        return Err(Error::internal(
            "read/write split is not supported for in-memory SQLite databases",
        ));
    }

    let url = build_url(&config.path)?;
    let reader_pool = build_sqlite_pool(&url, config, Some(&config.reader)).await?;
    let writer_pool = build_sqlite_pool(&url, config, Some(&config.writer)).await?;

    Ok((ReadPool::new(reader_pool), WritePool::new(writer_pool)))
}

#[cfg(feature = "sqlite")]
fn build_url(path: &str) -> Result<String> {
    if path == ":memory:" {
        return Ok("sqlite::memory:".to_string());
    }

    let path = std::path::Path::new(path);
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
        && !parent.exists()
    {
        std::fs::create_dir_all(parent)
            .map_err(|e| Error::internal(format!("failed to create database directory: {e}")))?;
    }

    Ok(format!("sqlite://{}?mode=rwc", path.display()))
}

#[cfg(feature = "sqlite")]
async fn build_sqlite_pool(
    url: &str,
    config: &SqliteConfig,
    overrides: Option<&super::config::PoolOverrides>,
) -> Result<sqlx::SqlitePool> {
    use sqlx::sqlite::SqlitePoolOptions;

    let max_conn = overrides
        .and_then(|o| o.max_connections)
        .unwrap_or(config.max_connections);
    let min_conn = overrides
        .and_then(|o| o.min_connections)
        .unwrap_or(config.min_connections);
    let acquire_timeout = overrides
        .and_then(|o| o.acquire_timeout_secs)
        .unwrap_or(config.acquire_timeout_secs);
    let idle_timeout = overrides
        .and_then(|o| o.idle_timeout_secs)
        .unwrap_or(config.idle_timeout_secs);
    let max_lifetime = overrides
        .and_then(|o| o.max_lifetime_secs)
        .unwrap_or(config.max_lifetime_secs);
    let busy_timeout = overrides
        .and_then(|o| o.busy_timeout)
        .unwrap_or(config.busy_timeout);
    let cache_size = overrides
        .and_then(|o| o.cache_size)
        .unwrap_or(config.cache_size);
    let mmap_size = overrides.and_then(|o| o.mmap_size).or(config.mmap_size);
    let temp_store = overrides.and_then(|o| o.temp_store).or(config.temp_store);
    let wal_autocheckpoint = overrides
        .and_then(|o| o.wal_autocheckpoint)
        .or(config.wal_autocheckpoint);

    let journal_mode = config.journal_mode;
    let synchronous = config.synchronous;
    let foreign_keys = config.foreign_keys;

    let pool = SqlitePoolOptions::new()
        .max_connections(max_conn)
        .min_connections(min_conn)
        .acquire_timeout(Duration::from_secs(acquire_timeout))
        .idle_timeout(Duration::from_secs(idle_timeout))
        .max_lifetime(Duration::from_secs(max_lifetime))
        .after_connect(move |conn, _meta| {
            Box::pin(async move {
                use sqlx::Executor;
                conn.execute(format!("PRAGMA journal_mode = {journal_mode}").as_str())
                    .await?;
                conn.execute(format!("PRAGMA busy_timeout = {busy_timeout}").as_str())
                    .await?;
                conn.execute(format!("PRAGMA synchronous = {synchronous}").as_str())
                    .await?;
                conn.execute(
                    format!(
                        "PRAGMA foreign_keys = {}",
                        if foreign_keys { "ON" } else { "OFF" }
                    )
                    .as_str(),
                )
                .await?;
                conn.execute(format!("PRAGMA cache_size = {cache_size}").as_str())
                    .await?;
                if let Some(mmap) = mmap_size {
                    conn.execute(format!("PRAGMA mmap_size = {mmap}").as_str())
                        .await?;
                }
                if let Some(ts) = temp_store {
                    conn.execute(format!("PRAGMA temp_store = {ts}").as_str())
                        .await?;
                }
                if let Some(ac) = wal_autocheckpoint {
                    conn.execute(format!("PRAGMA wal_autocheckpoint = {ac}").as_str())
                        .await?;
                }
                Ok(())
            })
        })
        .connect(url)
        .await
        .map_err(|e| Error::internal(format!("failed to connect to database: {e}")))?;

    Ok(pool)
}
