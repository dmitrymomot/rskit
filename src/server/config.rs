use serde::Deserialize;

/// HTTP server configuration.
///
/// Deserialized from the `server` section of the application YAML config file.
/// All fields have sane defaults so the section may be omitted entirely.
///
/// # YAML example
///
/// ```yaml
/// server:
///   host: 0.0.0.0
///   port: ${PORT:8080}
///   shutdown_timeout_secs: 30
/// ```
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Network interface to bind. Defaults to `"localhost"`.
    pub host: String,
    /// TCP port to listen on. Defaults to `8080`.
    pub port: u16,
    /// Maximum seconds to wait for in-flight requests to complete during
    /// graceful shutdown. Defaults to `30`.
    pub shutdown_timeout_secs: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            host: "localhost".to_string(),
            port: 8080,
            shutdown_timeout_secs: 30,
        }
    }
}
