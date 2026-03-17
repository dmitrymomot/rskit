use serde::Deserialize;
use std::fmt;

/// SQLite WAL journal mode variant.
#[derive(Debug, Clone, Copy, Deserialize, Default)]
#[serde(rename_all = "UPPERCASE")]
pub enum JournalMode {
    /// Write-Ahead Logging — best for concurrent reads and writes.
    #[default]
    Wal,
    /// Classic rollback journal, one writer at a time.
    Delete,
    /// Rollback journal truncated instead of deleted on commit.
    Truncate,
    /// Rollback journal persisted (zeroed header) between transactions.
    Persist,
    /// No journal — no rollback on crash.
    Off,
}

impl fmt::Display for JournalMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Wal => write!(f, "WAL"),
            Self::Delete => write!(f, "DELETE"),
            Self::Truncate => write!(f, "TRUNCATE"),
            Self::Persist => write!(f, "PERSIST"),
            Self::Off => write!(f, "OFF"),
        }
    }
}

/// SQLite synchronous PRAGMA value.
#[derive(Debug, Clone, Copy, Deserialize, Default)]
#[serde(rename_all = "UPPERCASE")]
pub enum SynchronousMode {
    /// Maximum durability — fsync on every write.
    Full,
    /// Good balance of safety and performance (default).
    #[default]
    Normal,
    /// No fsync — fastest but unsafe on power loss.
    Off,
}

impl fmt::Display for SynchronousMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Full => write!(f, "FULL"),
            Self::Normal => write!(f, "NORMAL"),
            Self::Off => write!(f, "OFF"),
        }
    }
}

/// Where SQLite stores temporary tables and indices.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum TempStore {
    /// Use the compile-time default (usually FILE).
    Default,
    /// Store temp data on disk.
    File,
    /// Store temp data in memory.
    Memory,
}

impl fmt::Display for TempStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Default => write!(f, "DEFAULT"),
            Self::File => write!(f, "FILE"),
            Self::Memory => write!(f, "MEMORY"),
        }
    }
}

/// Per-pool overrides for `connect_rw()`.
///
/// All fields are optional and fall back to the top-level [`SqliteConfig`] values when `None`.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct PoolOverrides {
    /// Override the maximum number of connections in the pool.
    pub max_connections: Option<u32>,
    /// Override the minimum number of connections in the pool.
    pub min_connections: Option<u32>,
    /// Override the acquire timeout in seconds.
    pub acquire_timeout_secs: Option<u64>,
    /// Override the idle timeout in seconds.
    pub idle_timeout_secs: Option<u64>,
    /// Override the maximum connection lifetime in seconds.
    pub max_lifetime_secs: Option<u64>,
    /// Override the SQLite `busy_timeout` PRAGMA (milliseconds).
    pub busy_timeout: Option<u32>,
    /// Override the SQLite `cache_size` PRAGMA (negative = KiB).
    pub cache_size: Option<i32>,
    /// Override the SQLite `mmap_size` PRAGMA (bytes).
    pub mmap_size: Option<i64>,
    /// Override the SQLite `temp_store` PRAGMA.
    pub temp_store: Option<TempStore>,
    /// Override the WAL auto-checkpoint threshold (pages).
    pub wal_autocheckpoint: Option<u32>,
}

/// Configuration for a SQLite connection pool.
///
/// Supports optional per-pool overrides via [`reader`](Self::reader) and
/// [`writer`](Self::writer) for read/write split workloads.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct SqliteConfig {
    /// Path to the SQLite database file. Defaults to `"data/app.db"`.
    pub path: String,
    /// Maximum number of connections in the pool. Defaults to `10`.
    pub max_connections: u32,
    /// Minimum number of idle connections kept open. Defaults to `1`.
    pub min_connections: u32,
    /// Seconds to wait for a connection from the pool. Defaults to `30`.
    pub acquire_timeout_secs: u64,
    /// Seconds a connection may sit idle before being closed. Defaults to `600`.
    pub idle_timeout_secs: u64,
    /// Maximum lifetime of a connection in seconds. Defaults to `1800`.
    pub max_lifetime_secs: u64,
    /// SQLite `journal_mode` PRAGMA. Defaults to [`JournalMode::Wal`].
    pub journal_mode: JournalMode,
    /// SQLite `busy_timeout` PRAGMA in milliseconds. Defaults to `5000`.
    pub busy_timeout: u32,
    /// SQLite `synchronous` PRAGMA. Defaults to [`SynchronousMode::Normal`].
    pub synchronous: SynchronousMode,
    /// Whether to enable foreign key enforcement. Defaults to `true`.
    pub foreign_keys: bool,
    /// SQLite `cache_size` PRAGMA (negative = KiB). Defaults to `-2000` (2 MiB).
    pub cache_size: i32,
    /// SQLite `mmap_size` PRAGMA in bytes. Defaults to `None` (opt-in).
    pub mmap_size: Option<i64>,
    /// SQLite `temp_store` PRAGMA. Defaults to `None` (opt-in).
    pub temp_store: Option<TempStore>,
    /// WAL auto-checkpoint threshold in pages. Defaults to `None` (SQLite default).
    pub wal_autocheckpoint: Option<u32>,
    /// Per-pool overrides applied to the reader pool in read/write split mode.
    pub reader: PoolOverrides,
    /// Per-pool overrides applied to the writer pool in read/write split mode.
    pub writer: PoolOverrides,
}

impl Default for SqliteConfig {
    fn default() -> Self {
        Self {
            path: String::from("data/app.db"),
            max_connections: 10,
            min_connections: 1,
            acquire_timeout_secs: 30,
            idle_timeout_secs: 600,
            max_lifetime_secs: 1800,
            journal_mode: JournalMode::Wal,
            busy_timeout: 5000,
            synchronous: SynchronousMode::Normal,
            foreign_keys: true,
            cache_size: -2000,
            mmap_size: None,
            temp_store: None,
            wal_autocheckpoint: None,
            reader: PoolOverrides {
                busy_timeout: Some(1000),
                cache_size: Some(-16000),
                mmap_size: Some(268_435_456),
                ..Default::default()
            },
            writer: PoolOverrides {
                max_connections: Some(1),
                busy_timeout: Some(2000),
                cache_size: Some(-16000),
                mmap_size: Some(268_435_456),
                ..Default::default()
            },
        }
    }
}
