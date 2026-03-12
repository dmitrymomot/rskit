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
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct SseConfig {
    /// Keep-alive interval in seconds. The server sends a comment line (`:`)
    /// at this interval to prevent proxies and browsers from closing idle
    /// connections.
    ///
    /// Converted to [`Duration`] via [`keep_alive_interval()`](Self::keep_alive_interval).
    #[serde(default = "default_keep_alive_interval_secs")]
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
