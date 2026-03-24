use serde::Deserialize;
use std::time::Duration;

fn default_keep_alive_interval_secs() -> u64 {
    15
}

/// Configuration for Server-Sent Events.
///
/// Controls keep-alive behavior for SSE connections. Loaded from the `sse`
/// section of your application config YAML.
///
/// # Example
///
/// ```yaml
/// sse:
///     keep_alive_interval_secs: 30
/// ```
///
/// # Defaults
///
/// | Field | Default |
/// |-------|---------|
/// | `keep_alive_interval_secs` | `15` |
#[non_exhaustive]
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct SseConfig {
    /// Keep-alive interval in seconds. The server sends a comment line (`:`)
    /// at this interval to prevent proxies and browsers from closing idle
    /// connections.
    pub keep_alive_interval_secs: u64,
}

impl Default for SseConfig {
    fn default() -> Self {
        Self {
            keep_alive_interval_secs: default_keep_alive_interval_secs(),
        }
    }
}

impl SseConfig {
    /// Returns the keep-alive interval as a [`Duration`].
    pub fn keep_alive_interval(&self) -> Duration {
        Duration::from_secs(self.keep_alive_interval_secs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_keep_alive_is_15_seconds() {
        let config = SseConfig::default();
        assert_eq!(config.keep_alive_interval_secs, 15);
        assert_eq!(
            config.keep_alive_interval(),
            std::time::Duration::from_secs(15)
        );
    }

    #[test]
    fn deserialize_from_yaml() {
        let yaml = "keep_alive_interval_secs: 30";
        let config: SseConfig = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(config.keep_alive_interval_secs, 30);
        assert_eq!(
            config.keep_alive_interval(),
            std::time::Duration::from_secs(30)
        );
    }

    #[test]
    fn deserialize_empty_uses_defaults() {
        let yaml = "{}";
        let config: SseConfig = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(config.keep_alive_interval_secs, 15);
    }
}
