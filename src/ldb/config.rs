use serde::Deserialize;

/// Database configuration. All fields have sensible defaults.
#[derive(Debug, Deserialize)]
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
        }
    }
}

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
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "OFF",
            Self::Normal => "NORMAL",
            Self::Full => "FULL",
            Self::Extra => "EXTRA",
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum TempStore {
    Default,
    File,
    #[default]
    Memory,
}

impl TempStore {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Default => "DEFAULT",
            Self::File => "FILE",
            Self::Memory => "MEMORY",
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
}
