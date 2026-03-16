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
    /// Seconds to wait for a connection from the pool before timing out (default: 30).
    pub acquire_timeout_secs: u64,
    /// Seconds a connection may sit idle in the pool before being closed (default: 600).
    pub idle_timeout_secs: u64,
    /// Maximum lifetime of a connection in seconds before it is recycled (default: 1800).
    pub max_lifetime_secs: u64,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            url: "sqlite://data/main.db?mode=rwc".to_string(),
            max_connections: 5,
            min_connections: 1,
            acquire_timeout_secs: 30,
            idle_timeout_secs: 600,
            max_lifetime_secs: 1800,
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
url: "postgres://localhost/test"
acquire_timeout_secs: 10
"#;
        let config: DatabaseConfig = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(config.url, "postgres://localhost/test");
        assert_eq!(config.acquire_timeout_secs, 10);
        // defaults for omitted fields
        assert_eq!(config.idle_timeout_secs, 600);
        assert_eq!(config.max_lifetime_secs, 1800);
        assert_eq!(config.max_connections, 5);
        assert_eq!(config.min_connections, 1);
    }
}
