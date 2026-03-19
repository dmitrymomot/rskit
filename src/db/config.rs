use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct SqliteConfig {
    pub path: String,
    pub max_connections: u32,
    pub min_connections: u32,
    pub acquire_timeout_secs: u64,
    pub idle_timeout_secs: u64,
    pub max_lifetime_secs: u64,
    pub journal_mode: JournalMode,
    pub synchronous: SynchronousMode,
    pub foreign_keys: bool,
    pub busy_timeout: u64,
    pub cache_size: i64,
    pub mmap_size: Option<u64>,
    pub temp_store: Option<TempStore>,
    pub wal_autocheckpoint: Option<u32>,
    pub reader: PoolOverrides,
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

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum JournalMode {
    Delete,
    Truncate,
    Persist,
    Memory,
    Wal,
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

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum SynchronousMode {
    Off,
    Normal,
    Full,
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

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum TempStore {
    Default,
    File,
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

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct PoolOverrides {
    pub max_connections: Option<u32>,
    pub min_connections: Option<u32>,
    pub acquire_timeout_secs: Option<u64>,
    pub idle_timeout_secs: Option<u64>,
    pub max_lifetime_secs: Option<u64>,
    pub busy_timeout: Option<u64>,
    pub cache_size: Option<i64>,
    pub mmap_size: Option<u64>,
    pub temp_store: Option<TempStore>,
    pub wal_autocheckpoint: Option<u32>,
}

impl PoolOverrides {
    pub fn default_reader() -> Self {
        Self {
            busy_timeout: Some(1000),
            cache_size: Some(-16000),
            mmap_size: Some(268_435_456),
            ..Default::default()
        }
    }

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
