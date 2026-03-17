use sqlx::sqlite::{SqliteConnectOptions, SqliteConnection, SqlitePoolOptions};
use std::str::FromStr;

use crate::{
    config::{JournalMode, PoolOverrides, SqliteConfig, SynchronousMode, TempStore},
    pool::{Pool, ReadPool, WritePool},
};

/// Builds the SQLite connection URL from the given path.
///
/// - `:memory:` → `sqlite::memory:`
/// - Any other path → creates parent directories and returns `sqlite://{path}?mode=rwc`
fn build_url(path: &str) -> Result<String, crate::Error> {
    if path == ":memory:" {
        return Ok("sqlite::memory:".to_string());
    }

    // Create parent directories if they don't exist
    if let Some(parent) = std::path::Path::new(path).parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).map_err(|e| crate::Error::Query(sqlx::Error::Io(e)))?;
    }

    Ok(format!("sqlite://{}?mode=rwc", path))
}

/// Parameters for building a connection pool.
struct PoolParams {
    url: String,
    max_connections: u32,
    min_connections: u32,
    acquire_timeout_secs: u64,
    idle_timeout_secs: u64,
    max_lifetime_secs: u64,
    journal_mode: JournalMode,
    busy_timeout: u32,
    synchronous: SynchronousMode,
    foreign_keys: bool,
    cache_size: i32,
    mmap_size: Option<i64>,
    temp_store: Option<TempStore>,
    wal_autocheckpoint: Option<u32>,
}

/// Builds a raw `sqlx::SqlitePool` from the given parameters.
async fn build_pool(params: PoolParams) -> Result<sqlx::SqlitePool, sqlx::Error> {
    let options = SqliteConnectOptions::from_str(&params.url)?;

    // Clone/copy all values that need to be moved into the 'static closure
    let journal_mode = params.journal_mode.to_string();
    let busy_timeout = params.busy_timeout;
    let synchronous = params.synchronous.to_string();
    let foreign_keys = params.foreign_keys;
    let cache_size = params.cache_size;
    let mmap_size = params.mmap_size;
    let temp_store = params.temp_store.map(|ts| ts.to_string());
    let wal_autocheckpoint = params.wal_autocheckpoint;

    let pool = SqlitePoolOptions::new()
        .max_connections(params.max_connections)
        .min_connections(params.min_connections)
        .acquire_timeout(std::time::Duration::from_secs(params.acquire_timeout_secs))
        .idle_timeout(std::time::Duration::from_secs(params.idle_timeout_secs))
        .max_lifetime(std::time::Duration::from_secs(params.max_lifetime_secs))
        .after_connect(move |conn: &mut SqliteConnection, _meta| {
            let journal_mode = journal_mode.clone();
            let synchronous = synchronous.clone();
            let temp_store = temp_store.clone();
            Box::pin(async move {
                sqlx::query(&format!("PRAGMA journal_mode = {journal_mode}"))
                    .execute(&mut *conn)
                    .await?;
                sqlx::query(&format!("PRAGMA busy_timeout = {busy_timeout}"))
                    .execute(&mut *conn)
                    .await?;
                sqlx::query(&format!("PRAGMA synchronous = {synchronous}"))
                    .execute(&mut *conn)
                    .await?;
                sqlx::query(&format!(
                    "PRAGMA foreign_keys = {}",
                    if foreign_keys { 1 } else { 0 }
                ))
                .execute(&mut *conn)
                .await?;
                sqlx::query(&format!("PRAGMA cache_size = {cache_size}"))
                    .execute(&mut *conn)
                    .await?;
                if let Some(mmap) = mmap_size {
                    sqlx::query(&format!("PRAGMA mmap_size = {mmap}"))
                        .execute(&mut *conn)
                        .await?;
                }
                if let Some(ref ts) = temp_store {
                    sqlx::query(&format!("PRAGMA temp_store = {ts}"))
                        .execute(&mut *conn)
                        .await?;
                }
                if let Some(wal_ac) = wal_autocheckpoint {
                    sqlx::query(&format!("PRAGMA wal_autocheckpoint = {wal_ac}"))
                        .execute(&mut *conn)
                        .await?;
                }
                Ok(())
            })
        })
        .connect_with(options)
        .await?;

    Ok(pool)
}

/// Opens a single general-purpose SQLite connection pool.
///
/// All PRAGMAs are applied to every connection on connect, using the top-level
/// values in `config`. Works with `:memory:` databases.
///
/// # Errors
///
/// Returns [`crate::Error`] if the database file cannot be created, the URL is
/// invalid, or the pool cannot connect.
pub async fn connect(config: &SqliteConfig) -> Result<Pool, crate::Error> {
    let url = build_url(&config.path)?;

    let params = PoolParams {
        url,
        max_connections: config.max_connections,
        min_connections: config.min_connections,
        acquire_timeout_secs: config.acquire_timeout_secs,
        idle_timeout_secs: config.idle_timeout_secs,
        max_lifetime_secs: config.max_lifetime_secs,
        journal_mode: config.journal_mode,
        busy_timeout: config.busy_timeout,
        synchronous: config.synchronous,
        foreign_keys: config.foreign_keys,
        cache_size: config.cache_size,
        mmap_size: config.mmap_size,
        temp_store: config.temp_store,
        wal_autocheckpoint: config.wal_autocheckpoint,
    };

    let pool = build_pool(params).await?;
    Ok(Pool(pool))
}

/// Opens a read/write split pair of SQLite connection pools.
///
/// Returns `(ReadPool, WritePool)`. Per-pool overrides in `config.reader` and
/// `config.writer` take precedence over the top-level values for their
/// respective pools.
///
/// # Errors
///
/// Returns [`crate::Error::Query`] with a configuration error if `path` is
/// `:memory:` — in-memory databases cannot be shared between two pools.
///
/// Also returns an error if pool creation fails for either pool.
pub async fn connect_rw(config: &SqliteConfig) -> Result<(ReadPool, WritePool), crate::Error> {
    if config.path == ":memory:" {
        return Err(crate::Error::Query(sqlx::Error::Configuration(
            "connect_rw() does not support :memory: databases — \
             in-memory databases cannot be shared between two separate pools"
                .into(),
        )));
    }

    let url = build_url(&config.path)?;

    let reader_params = pool_params_with_overrides(&url, config, &config.reader);
    let writer_params = pool_params_with_overrides(&url, config, &config.writer);

    let reader_pool = build_pool(reader_params).await?;
    let writer_pool = build_pool(writer_params).await?;

    Ok((ReadPool(reader_pool), WritePool(writer_pool)))
}

/// Builds [`PoolParams`] by merging top-level `config` values with per-pool `overrides`.
fn pool_params_with_overrides(
    url: &str,
    config: &SqliteConfig,
    overrides: &PoolOverrides,
) -> PoolParams {
    PoolParams {
        url: url.to_string(),
        max_connections: overrides.max_connections.unwrap_or(config.max_connections),
        min_connections: overrides.min_connections.unwrap_or(config.min_connections),
        acquire_timeout_secs: overrides
            .acquire_timeout_secs
            .unwrap_or(config.acquire_timeout_secs),
        idle_timeout_secs: overrides
            .idle_timeout_secs
            .unwrap_or(config.idle_timeout_secs),
        max_lifetime_secs: overrides
            .max_lifetime_secs
            .unwrap_or(config.max_lifetime_secs),
        journal_mode: config.journal_mode,
        busy_timeout: overrides.busy_timeout.unwrap_or(config.busy_timeout),
        synchronous: config.synchronous,
        foreign_keys: config.foreign_keys,
        cache_size: overrides.cache_size.unwrap_or(config.cache_size),
        mmap_size: overrides.mmap_size.or(config.mmap_size),
        temp_store: overrides.temp_store.or(config.temp_store),
        wal_autocheckpoint: overrides.wal_autocheckpoint.or(config.wal_autocheckpoint),
    }
}
