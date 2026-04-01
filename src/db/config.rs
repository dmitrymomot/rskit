use serde::Deserialize;

/// Database configuration with sensible defaults for SQLite/libsql.
///
/// All fields are optional when deserializing from YAML. Defaults produce
/// a WAL-mode database at `data/app.db` with foreign keys enabled.
///
/// If [`migrations`](Self::migrations) is set, SQL migrations from that
/// directory are applied automatically on [`connect`](super::connect).
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    /// Database file path.
    #[serde(default = "defaults::path")]
    pub path: String,

    /// Migration directory. If set, migrations run on connect.
    #[serde(default)]
    pub migrations: Option<String>,

    /// Busy timeout in milliseconds.
    #[serde(default = "defaults::busy_timeout")]
    pub busy_timeout: u64,

    /// Cache size in KB (applied as PRAGMA cache_size = -N).
    #[serde(default = "defaults::cache_size")]
    pub cache_size: i64,

    /// Memory-mapped I/O size in bytes.
    #[serde(default = "defaults::mmap_size")]
    pub mmap_size: u64,

    /// WAL journal mode.
    #[serde(default = "defaults::journal_mode")]
    pub journal_mode: JournalMode,

    /// Synchronous mode.
    #[serde(default = "defaults::synchronous")]
    pub synchronous: SynchronousMode,

    /// Foreign key enforcement.
    #[serde(default = "defaults::foreign_keys")]
    pub foreign_keys: bool,

    /// Temp store location.
    #[serde(default = "defaults::temp_store")]
    pub temp_store: TempStore,

    /// Optional pool configuration for multi-database sharding.
    /// When set, [`DatabasePool::new`](super::DatabasePool::new) can be used
    /// to manage shard databases that share this config's PRAGMAs and migrations.
    #[serde(default)]
    pub pool: Option<PoolConfig>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            path: defaults::path(),
            migrations: None,
            busy_timeout: defaults::busy_timeout(),
            cache_size: defaults::cache_size(),
            mmap_size: defaults::mmap_size(),
            journal_mode: defaults::journal_mode(),
            synchronous: defaults::synchronous(),
            foreign_keys: defaults::foreign_keys(),
            temp_store: defaults::temp_store(),
            pool: None,
        }
    }
}

/// SQLite journal mode.
///
/// Controls how the database writes transactions. Default is [`Wal`](Self::Wal).
#[derive(Debug, Clone, Copy, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum JournalMode {
    #[default]
    Wal,
    Delete,
    Truncate,
    Memory,
    Off,
}

impl JournalMode {
    /// Returns the PRAGMA-compatible string representation.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Wal => "WAL",
            Self::Delete => "DELETE",
            Self::Truncate => "TRUNCATE",
            Self::Memory => "MEMORY",
            Self::Off => "OFF",
        }
    }
}

/// SQLite synchronous mode.
///
/// Controls the trade-off between durability and write performance.
/// Default is [`Normal`](Self::Normal).
#[derive(Debug, Clone, Copy, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SynchronousMode {
    Off,
    #[default]
    Normal,
    Full,
    Extra,
}

impl SynchronousMode {
    /// Returns the PRAGMA-compatible string representation.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "OFF",
            Self::Normal => "NORMAL",
            Self::Full => "FULL",
            Self::Extra => "EXTRA",
        }
    }
}

/// SQLite temp store location.
///
/// Controls where temporary tables and indices are stored.
/// Default is [`Memory`](Self::Memory).
#[derive(Debug, Clone, Copy, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum TempStore {
    Default,
    File,
    #[default]
    Memory,
}

impl TempStore {
    /// Returns the PRAGMA-compatible string representation.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Default => "DEFAULT",
            Self::File => "FILE",
            Self::Memory => "MEMORY",
        }
    }
}

/// Pool configuration for multi-database sharding.
///
/// When nested inside [`Config`], enables [`DatabasePool`](super::DatabasePool)
/// to manage lazily-opened shard databases that share the parent config's
/// PRAGMAs and migrations.
#[derive(Debug, Clone, Deserialize)]
pub struct PoolConfig {
    /// Directory where shard databases are stored.
    /// Each shard creates `{base_path}/{shard_name}.db`.
    #[serde(default = "defaults::base_path")]
    pub base_path: String,

    /// Number of internal lock shards for the connection map.
    /// Controls lock contention parallelism, not the number of tenant databases.
    #[serde(default = "defaults::lock_shards")]
    pub lock_shards: usize,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            base_path: defaults::base_path(),
            lock_shards: defaults::lock_shards(),
        }
    }
}

mod defaults {
    use super::*;

    pub fn path() -> String {
        "data/app.db".to_string()
    }

    pub fn busy_timeout() -> u64 {
        5000
    }

    pub fn cache_size() -> i64 {
        16384
    }

    pub fn mmap_size() -> u64 {
        268_435_456 // 256 MB
    }

    pub fn journal_mode() -> JournalMode {
        JournalMode::Wal
    }

    pub fn synchronous() -> SynchronousMode {
        SynchronousMode::Normal
    }

    pub fn foreign_keys() -> bool {
        true
    }

    pub fn temp_store() -> TempStore {
        TempStore::Memory
    }

    pub fn base_path() -> String {
        "data/shards".to_string()
    }

    pub fn lock_shards() -> usize {
        16
    }
}
