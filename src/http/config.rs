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
    /// Default request timeout in seconds. `0` means no timeout.
    pub timeout_secs: u64,
    /// TCP connect timeout in seconds.
    pub connect_timeout_secs: u64,
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
            timeout_secs: 30,
            connect_timeout_secs: 5,
            user_agent: "modo/0.1".to_string(),
            max_retries: 0,
            retry_backoff_ms: 100,
        }
    }
}

#[allow(dead_code)]
impl ClientConfig {
    /// Request timeout as a `Duration`. Returns `None` when `timeout_secs` is `0`.
    pub(crate) fn timeout(&self) -> Option<Duration> {
        if self.timeout_secs == 0 {
            None
        } else {
            Some(Duration::from_secs(self.timeout_secs))
        }
    }

    /// TCP connect timeout as a `Duration`.
    pub(crate) fn connect_timeout(&self) -> Duration {
        Duration::from_secs(self.connect_timeout_secs)
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
        assert_eq!(config.timeout_secs, 30);
        assert_eq!(config.connect_timeout_secs, 5);
        assert_eq!(config.user_agent, "modo/0.1");
        assert_eq!(config.max_retries, 0);
        assert_eq!(config.retry_backoff_ms, 100);
    }

    #[test]
    fn timeout_returns_none_for_zero() {
        let config = ClientConfig {
            timeout_secs: 0,
            ..Default::default()
        };
        assert!(config.timeout().is_none());
    }

    #[test]
    fn timeout_returns_duration_for_nonzero() {
        let config = ClientConfig::default();
        assert_eq!(config.timeout(), Some(Duration::from_secs(30)));
    }

    #[test]
    fn deserialize_from_yaml() {
        let yaml = r#"
timeout_secs: 10
connect_timeout_secs: 2
user_agent: "myapp/1.0"
max_retries: 3
retry_backoff_ms: 200
"#;
        let config: ClientConfig = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(config.timeout_secs, 10);
        assert_eq!(config.connect_timeout_secs, 2);
        assert_eq!(config.user_agent, "myapp/1.0");
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.retry_backoff_ms, 200);
    }

    #[test]
    fn deserialize_empty_yaml_uses_defaults() {
        let config: ClientConfig = serde_yaml_ng::from_str("{}").unwrap();
        assert_eq!(config.timeout_secs, 30);
        assert_eq!(config.max_retries, 0);
    }
}
