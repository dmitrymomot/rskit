use serde::Deserialize;

/// Configuration for a SQLite connection pool.
///
/// Deserializes from YAML (or any serde format) with `${VAR:default}` env var
/// substitution applied by the config loader. All fields have sensible
/// defaults via [`Default`].
///
/// # Default values
///
/// | Field | Default |
/// |---|---|
/// | `path` | `"data/app.db"` |
/// | `max_connections` | `10` |
/// | `min_connections` | `1` |
/// | `acquire_timeout_secs` | `30` |
/// | `idle_timeout_secs` | `600` |
/// | `max_lifetime_secs` | `1800` |
/// | `journal_mode` | `WAL` |
/// | `synchronous` | `NORMAL` |
/// | `foreign_keys` | `true` |
/// | `busy_timeout` | `5000` ms |
/// | `cache_size` | `-2000` (2 MB) |
///
/// # Example YAML
///
/// ```yaml
/// database:
///   path: data/app.db
///   max_connections: 20
///   journal_mode: WAL
///   synchronous: NORMAL
///   foreign_keys: true
///   busy_timeout: 5000
/// ```
#[non_exhaustive]
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct SqliteConfig {
    /// Path to the SQLite database file. Use `":memory:"` for an in-memory database.
    pub path: String,
    /// Maximum number of connections in the pool.
    pub max_connections: u32,
    /// Minimum number of idle connections kept open.
    pub min_connections: u32,
    /// Seconds to wait before returning an error when acquiring a connection.
    pub acquire_timeout_secs: u64,
    /// Seconds a connection may sit idle before being closed.
    pub idle_timeout_secs: u64,
    /// Maximum lifetime of a connection in seconds.
    pub max_lifetime_secs: u64,
    /// SQLite `PRAGMA journal_mode` setting.
    pub journal_mode: JournalMode,
    /// SQLite `PRAGMA synchronous` setting.
    pub synchronous: SynchronousMode,
    /// Whether to enable `PRAGMA foreign_keys = ON`.
    pub foreign_keys: bool,
    /// SQLite `PRAGMA busy_timeout` in milliseconds.
    pub busy_timeout: u64,
    /// SQLite `PRAGMA cache_size`. Negative values are in KiB (e.g. `-2000` = 2 MB).
    pub cache_size: i64,
    /// SQLite `PRAGMA mmap_size` in bytes, if set.
    pub mmap_size: Option<u64>,
    /// SQLite `PRAGMA temp_store` setting, if set.
    pub temp_store: Option<TempStore>,
    /// SQLite `PRAGMA wal_autocheckpoint` page count, if set.
    pub wal_autocheckpoint: Option<u32>,
    /// Per-pool overrides applied to the reader pool when using `connect_rw`.
    pub reader: PoolOverrides,
    /// Per-pool overrides applied to the writer pool when using `connect_rw`.
    pub writer: PoolOverrides,
}

impl Default for SqliteConfig {
    fn default() -> Self {
        Self {
            path: "data/app.db".to_string(),
            max_connections: 10,
            min_connections: 1,
            acquire_timeout_secs: 30,
            idle_timeout_secs: 600,
            max_lifetime_secs: 1800,
            journal_mode: JournalMode::Wal,
            synchronous: SynchronousMode::Normal,
            foreign_keys: true,
            busy_timeout: 5000,
            cache_size: -2000,
            mmap_size: None,
            temp_store: None,
            wal_autocheckpoint: None,
            reader: PoolOverrides::default_reader(),
            writer: PoolOverrides::default_writer(),
        }
    }
}

/// SQLite `PRAGMA journal_mode` values.
///
/// Serializes/deserializes as uppercase strings (e.g. `"WAL"`, `"DELETE"`).
/// Displays as the SQLite PRAGMA value string.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum JournalMode {
    /// Rollback journal is deleted after each transaction.
    Delete,
    /// Rollback journal is truncated to zero length after each transaction.
    Truncate,
    /// Rollback journal is persisted between transactions.
    Persist,
    /// Rollback journal is stored in volatile memory.
    Memory,
    /// Write-ahead log mode (recommended for concurrent access).
    Wal,
    /// Journaling is disabled entirely (unsafe).
    Off,
}

impl std::fmt::Display for JournalMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Delete => write!(f, "DELETE"),
            Self::Truncate => write!(f, "TRUNCATE"),
            Self::Persist => write!(f, "PERSIST"),
            Self::Memory => write!(f, "MEMORY"),
            Self::Wal => write!(f, "WAL"),
            Self::Off => write!(f, "OFF"),
        }
    }
}

/// SQLite `PRAGMA synchronous` values.
///
/// Controls how aggressively SQLite syncs data to disk. `Normal` is the
/// recommended setting when WAL mode is used.
///
/// Serializes/deserializes as uppercase strings (e.g. `"NORMAL"`, `"FULL"`).
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum SynchronousMode {
    /// No sync calls; maximum performance, but data may be lost on crash.
    Off,
    /// Sync at critical moments; safe with WAL mode.
    Normal,
    /// Sync after every write; safest but slowest.
    Full,
    /// Sync after every write and also after committing a transaction.
    Extra,
}

impl std::fmt::Display for SynchronousMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Off => write!(f, "OFF"),
            Self::Normal => write!(f, "NORMAL"),
            Self::Full => write!(f, "FULL"),
            Self::Extra => write!(f, "EXTRA"),
        }
    }
}

/// SQLite `PRAGMA temp_store` values.
///
/// Controls where temporary tables and indices are stored.
///
/// Serializes/deserializes as uppercase strings (e.g. `"MEMORY"`, `"FILE"`).
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum TempStore {
    /// Use the compile-time default (usually file).
    Default,
    /// Store temporary objects on disk.
    File,
    /// Store temporary objects in memory.
    Memory,
}

impl std::fmt::Display for TempStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Default => write!(f, "DEFAULT"),
            Self::File => write!(f, "FILE"),
            Self::Memory => write!(f, "MEMORY"),
        }
    }
}

/// Per-pool PRAGMA overrides for read/write split configurations.
///
/// Fields are optional; any `None` field falls back to the corresponding
/// value in [`SqliteConfig`]. Used in [`SqliteConfig::reader`] and
/// [`SqliteConfig::writer`].
///
/// Construct sensible defaults via [`PoolOverrides::default_reader`] and
/// [`PoolOverrides::default_writer`].
#[non_exhaustive]
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct PoolOverrides {
    /// Override `max_connections` for this pool.
    pub max_connections: Option<u32>,
    /// Override `min_connections` for this pool.
    pub min_connections: Option<u32>,
    /// Override `acquire_timeout_secs` for this pool.
    pub acquire_timeout_secs: Option<u64>,
    /// Override `idle_timeout_secs` for this pool.
    pub idle_timeout_secs: Option<u64>,
    /// Override `max_lifetime_secs` for this pool.
    pub max_lifetime_secs: Option<u64>,
    /// Override `busy_timeout` (ms) for this pool.
    pub busy_timeout: Option<u64>,
    /// Override `cache_size` for this pool.
    pub cache_size: Option<i64>,
    /// Override `mmap_size` (bytes) for this pool.
    pub mmap_size: Option<u64>,
    /// Override `temp_store` for this pool.
    pub temp_store: Option<TempStore>,
    /// Override `wal_autocheckpoint` for this pool.
    pub wal_autocheckpoint: Option<u32>,
}

impl PoolOverrides {
    /// Default overrides for a read pool.
    ///
    /// Sets `busy_timeout = 1000` ms, `cache_size = -16000` (16 MB),
    /// and `mmap_size = 256 MiB`. `max_connections` is left as `None` so
    /// the base config value is used, allowing many concurrent readers.
    pub fn default_reader() -> Self {
        Self {
            busy_timeout: Some(1000),
            cache_size: Some(-16000),
            mmap_size: Some(268_435_456),
            ..Default::default()
        }
    }

    /// Default overrides for a write pool.
    ///
    /// Sets `max_connections = 1` to serialize writes, `busy_timeout = 2000` ms,
    /// `cache_size = -16000` (16 MB), and `mmap_size = 256 MiB`.
    pub fn default_writer() -> Self {
        Self {
            max_connections: Some(1),
            busy_timeout: Some(2000),
            cache_size: Some(-16000),
            mmap_size: Some(268_435_456),
            ..Default::default()
        }
    }
}
