use serde::Deserialize;
use std::fmt;

/// Database configuration, deserialized from YAML via `modo::config::load()`.
///
/// Backend is selected by setting either `sqlite` or `postgres` sub-config.
/// If neither is set, defaults to SQLite with `path: "data/main.db"`.
/// Setting both is an error (detected at connect time).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DatabaseConfig {
    /// Maximum number of connections in the pool.
    #[serde(default = "default_max_connections")]
    pub max_connections: u32,
    /// Minimum number of connections in the pool.
    #[serde(default = "default_min_connections")]
    pub min_connections: u32,
    /// Seconds to wait for a connection from the pool before timing out.
    #[serde(default = "default_acquire_timeout")]
    pub acquire_timeout_secs: u64,
    /// Seconds a connection may sit idle in the pool before being closed.
    #[serde(default = "default_idle_timeout")]
    pub idle_timeout_secs: u64,
    /// Maximum lifetime of a connection in seconds before it is closed and replaced.
    #[serde(default = "default_max_lifetime")]
    pub max_lifetime_secs: u64,
    /// SQLite-specific config. Presence selects SQLite backend.
    pub sqlite: Option<SqliteDbConfig>,
    /// Postgres-specific config. Presence selects Postgres backend.
    pub postgres: Option<PostgresDbConfig>,
}

fn default_max_connections() -> u32 {
    5
}
fn default_min_connections() -> u32 {
    1
}
fn default_acquire_timeout() -> u64 {
    30
}
fn default_idle_timeout() -> u64 {
    600
}
fn default_max_lifetime() -> u64 {
    1800
}
impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            max_connections: default_max_connections(),
            min_connections: default_min_connections(),
            acquire_timeout_secs: default_acquire_timeout(),
            idle_timeout_secs: default_idle_timeout(),
            max_lifetime_secs: default_max_lifetime(),
            sqlite: Some(SqliteDbConfig::default()),
            postgres: None,
        }
    }
}

/// SQLite-specific config. Presence of this section selects SQLite backend.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct SqliteDbConfig {
    /// Path to the SQLite database file (e.g., "data/main.db" or ":memory:").
    pub path: String,
    /// PRAGMA tuning settings applied per-connection.
    pub pragmas: SqliteConfig,
}

impl Default for SqliteDbConfig {
    fn default() -> Self {
        Self {
            path: "data/main.db".to_string(),
            pragmas: SqliteConfig::default(),
        }
    }
}

/// Postgres-specific config. Presence of this section selects Postgres backend.
#[derive(Debug, Clone, Deserialize)]
pub struct PostgresDbConfig {
    /// Full Postgres connection URL.
    pub url: String,
}

/// SQLite PRAGMA configuration applied to every connection in the pool.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct SqliteConfig {
    pub journal_mode: JournalMode,
    pub busy_timeout: u32,
    pub synchronous: SynchronousMode,
    pub foreign_keys: bool,
    pub cache_size: i32,
    pub mmap_size: Option<i64>,
    pub temp_store: Option<TempStore>,
    pub wal_autocheckpoint: Option<u32>,
}

impl Default for SqliteConfig {
    fn default() -> Self {
        Self {
            journal_mode: JournalMode::Wal,
            busy_timeout: 5000,
            synchronous: SynchronousMode::Normal,
            foreign_keys: true,
            cache_size: -2000,
            mmap_size: None,
            temp_store: None,
            wal_autocheckpoint: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Default)]
#[serde(rename_all = "UPPERCASE")]
pub enum JournalMode {
    #[default]
    Wal,
    Delete,
    Truncate,
    Persist,
    Memory,
    Off,
}

impl fmt::Display for JournalMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JournalMode::Wal => write!(f, "WAL"),
            JournalMode::Delete => write!(f, "DELETE"),
            JournalMode::Truncate => write!(f, "TRUNCATE"),
            JournalMode::Persist => write!(f, "PERSIST"),
            JournalMode::Memory => write!(f, "MEMORY"),
            JournalMode::Off => write!(f, "OFF"),
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Default)]
#[serde(rename_all = "UPPERCASE")]
pub enum SynchronousMode {
    Full,
    #[default]
    Normal,
    Off,
}

impl fmt::Display for SynchronousMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SynchronousMode::Full => write!(f, "FULL"),
            SynchronousMode::Normal => write!(f, "NORMAL"),
            SynchronousMode::Off => write!(f, "OFF"),
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

impl fmt::Display for TempStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TempStore::Default => write!(f, "DEFAULT"),
            TempStore::File => write!(f, "FILE"),
            TempStore::Memory => write!(f, "MEMORY"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_timeout_values() {
        let config = DatabaseConfig::default();
        assert_eq!(config.acquire_timeout_secs, 30);
        assert_eq!(config.idle_timeout_secs, 600);
        assert_eq!(config.max_lifetime_secs, 1800);
    }

    #[test]
    fn partial_yaml_deserialization() {
        let yaml = r#"
postgres:
    url: "postgres://localhost/test"
acquire_timeout_secs: 10
"#;
        let config: DatabaseConfig = serde_yaml_ng::from_str(yaml).unwrap();
        let pg = config.postgres.unwrap();
        assert_eq!(pg.url, "postgres://localhost/test");
        assert_eq!(config.acquire_timeout_secs, 10);
        // defaults for omitted fields
        assert_eq!(config.idle_timeout_secs, 600);
        assert_eq!(config.max_lifetime_secs, 1800);
        assert_eq!(config.max_connections, 5);
        assert_eq!(config.min_connections, 1);
    }
}
