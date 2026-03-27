use std::time::Duration;

use serde::Deserialize;

/// HTTP client configuration.
///
/// Deserializes from the `http:` section of the framework YAML config.
/// All fields have sensible defaults so the section can be omitted entirely.
#[non_exhaustive]
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ClientConfig {
    /// Default request timeout in milliseconds. `0` means no timeout.
    pub timeout_ms: u64,
    /// TCP connect timeout in milliseconds. `0` means no connect timeout.
    pub connect_timeout_ms: u64,
    /// Default `User-Agent` header value.
    pub user_agent: String,
    /// Maximum retry attempts for retryable failures. `0` means no retries.
    pub max_retries: u32,
    /// Initial backoff between retries in milliseconds.
    /// Actual backoff is `retry_backoff_ms * 2^attempt`.
    pub retry_backoff_ms: u64,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            timeout_ms: 30_000,
            connect_timeout_ms: 5_000,
            user_agent: "modo/0.1".to_string(),
            max_retries: 0,
            retry_backoff_ms: 100,
        }
    }
}

impl ClientConfig {
    /// Request timeout as a `Duration`. Returns `None` when `timeout_ms` is `0`.
    pub(crate) fn timeout(&self) -> Option<Duration> {
        if self.timeout_ms == 0 {
            None
        } else {
            Some(Duration::from_millis(self.timeout_ms))
        }
    }

    /// TCP connect timeout as a `Duration`. Returns `None` when `connect_timeout_ms` is `0`.
    pub(crate) fn connect_timeout(&self) -> Option<Duration> {
        if self.connect_timeout_ms == 0 {
            None
        } else {
            Some(Duration::from_millis(self.connect_timeout_ms))
        }
    }

    /// Retry backoff base as a `Duration`.
    pub(crate) fn retry_backoff(&self) -> Duration {
        Duration::from_millis(self.retry_backoff_ms)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_values() {
        let config = ClientConfig::default();
        assert_eq!(config.timeout_ms, 30_000);
        assert_eq!(config.connect_timeout_ms, 5_000);
        assert_eq!(config.user_agent, "modo/0.1");
        assert_eq!(config.max_retries, 0);
        assert_eq!(config.retry_backoff_ms, 100);
    }

    #[test]
    fn timeout_returns_none_for_zero() {
        let config = ClientConfig {
            timeout_ms: 0,
            ..Default::default()
        };
        assert!(config.timeout().is_none());
    }

    #[test]
    fn connect_timeout_returns_none_for_zero() {
        let config = ClientConfig {
            connect_timeout_ms: 0,
            ..Default::default()
        };
        assert!(config.connect_timeout().is_none());
    }

    #[test]
    fn timeout_returns_duration_for_nonzero() {
        let config = ClientConfig::default();
        assert_eq!(config.timeout(), Some(Duration::from_millis(30_000)));
    }

    #[test]
    fn connect_timeout_returns_duration_for_nonzero() {
        let config = ClientConfig::default();
        assert_eq!(config.connect_timeout(), Some(Duration::from_millis(5_000)));
    }

    #[test]
    fn deserialize_from_yaml() {
        let yaml = r#"
timeout_ms: 10000
connect_timeout_ms: 2000
user_agent: "myapp/1.0"
max_retries: 3
retry_backoff_ms: 200
"#;
        let config: ClientConfig = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(config.timeout_ms, 10_000);
        assert_eq!(config.connect_timeout_ms, 2_000);
        assert_eq!(config.user_agent, "myapp/1.0");
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.retry_backoff_ms, 200);
    }

    #[test]
    fn deserialize_empty_yaml_uses_defaults() {
        let config: ClientConfig = serde_yaml_ng::from_str("{}").unwrap();
        assert_eq!(config.timeout_ms, 30_000);
        assert_eq!(config.max_retries, 0);
    }
}
