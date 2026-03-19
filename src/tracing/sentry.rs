use crate::error::Result;
use crate::runtime::Task;

#[cfg(feature = "sentry")]
use serde::Deserialize;

#[cfg(feature = "sentry")]
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct SentryConfig {
    pub dsn: String,
    pub environment: String,
    pub sample_rate: f32,
    pub traces_sample_rate: f32,
}

#[cfg(feature = "sentry")]
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

pub struct TracingGuard {
    #[cfg(feature = "sentry")]
    _sentry: Option<sentry::ClientInitGuard>,
}

impl Default for TracingGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl TracingGuard {
    pub fn new() -> Self {
        Self {
            #[cfg(feature = "sentry")]
            _sentry: None,
        }
    }

    #[cfg(feature = "sentry")]
    pub fn with_sentry(guard: sentry::ClientInitGuard) -> Self {
        Self {
            _sentry: Some(guard),
        }
    }
}

impl Task for TracingGuard {
    async fn shutdown(self) -> Result<()> {
        #[cfg(feature = "sentry")]
        if let Some(guard) = self._sentry {
            guard.close(Some(std::time::Duration::from_secs(5)));
        }
        Ok(())
    }
}
