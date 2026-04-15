use crate::error::Result;
use crate::runtime::Task;

use serde::Deserialize;

/// Sentry error and performance reporting settings.
///
/// Embed in `Config::sentry` and supply a valid DSN to enable Sentry.
///
/// ```yaml
/// tracing:
///   sentry:
///     dsn: "https://key@sentry.io/project"
///     environment: production
///     sample_rate: 1.0
///     traces_sample_rate: 0.1
/// ```
#[non_exhaustive]
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct SentryConfig {
    /// Sentry DSN. When empty, Sentry is not initialised.
    pub dsn: String,

    /// Environment tag reported to Sentry (e.g. `"production"`).
    ///
    /// Defaults to the value of `APP_ENV` (see [`crate::config::env`]).
    pub environment: String,

    /// Fraction of error events to send (0.0–1.0). Defaults to `1.0`.
    pub sample_rate: f32,

    /// Fraction of transactions to trace for performance monitoring (0.0–1.0).
    /// Defaults to `0.1`.
    pub traces_sample_rate: f32,
}

impl Default for SentryConfig {
    fn default() -> Self {
        Self {
            dsn: String::new(),
            environment: crate::config::env(),
            sample_rate: 1.0,
            traces_sample_rate: 0.1,
        }
    }
}

/// RAII guard that keeps the tracing subscriber and Sentry client alive.
///
/// Returned by [`crate::tracing::init`]. Hold this value for the entire
/// lifetime of the process — typically by passing it to the `run!` macro
/// or calling [`Task::shutdown`] at the end of `main`.
///
/// Dropping the guard without calling `shutdown` is safe but may not flush
/// all buffered Sentry events.
pub struct TracingGuard {
    _sentry: Option<sentry::ClientInitGuard>,
}

impl Default for TracingGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl TracingGuard {
    /// Create a guard with no active Sentry client.
    pub fn new() -> Self {
        Self { _sentry: None }
    }

    /// Create a guard that owns an active Sentry client.
    pub fn with_sentry(guard: sentry::ClientInitGuard) -> Self {
        Self {
            _sentry: Some(guard),
        }
    }
}

impl Task for TracingGuard {
    /// Flush pending Sentry events (up to 5 seconds) and release the client.
    async fn shutdown(self) -> Result<()> {
        if let Some(guard) = self._sentry {
            guard.close(Some(std::time::Duration::from_secs(5)));
        }
        Ok(())
    }
}
