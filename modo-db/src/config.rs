use serde::Deserialize;

/// Database configuration, deserialized from YAML via `modo::config::load()`.
///
/// Backend is auto-detected from the URL scheme (`sqlite://` or `postgres://`).
/// Irrelevant fields are silently ignored for the active backend.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct DatabaseConfig {
    /// Connection URL (e.g., `sqlite://data.db?mode=rwc` or `postgres://localhost/myapp`).
    pub url: String,
    /// Maximum number of connections in the pool.
    pub max_connections: u32,
    /// Minimum number of connections in the pool.
    pub min_connections: u32,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            url: "sqlite://data/main.db?mode=rwc".to_string(),
            max_connections: 5,
            min_connections: 1,
        }
    }
}
