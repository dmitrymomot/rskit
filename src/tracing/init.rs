use serde::Deserialize;
use tracing_subscriber::prelude::*;
use tracing_subscriber::{EnvFilter, fmt};

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Config {
    pub level: String,
    pub format: String,
    #[cfg(feature = "sentry")]
    pub sentry: Option<super::sentry::SentryConfig>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
            format: "pretty".to_string(),
            #[cfg(feature = "sentry")]
            sentry: None,
        }
    }
}

pub fn init(config: &Config) -> crate::error::Result<super::sentry::TracingGuard> {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&config.level));

    #[cfg(feature = "sentry")]
    let sentry_guard = init_sentry(config);

    match config.format.as_str() {
        "json" => {
            let base = tracing_subscriber::registry()
                .with(filter)
                .with(fmt::layer().json());
            #[cfg(feature = "sentry")]
            {
                base.with(sentry_guard.as_ref().map(|_| sentry_tracing::layer()))
                    .try_init()
                    .ok();
            }
            #[cfg(not(feature = "sentry"))]
            {
                base.try_init().ok();
            }
        }
        "pretty" => {
            let base = tracing_subscriber::registry()
                .with(filter)
                .with(fmt::layer().pretty());
            #[cfg(feature = "sentry")]
            {
                base.with(sentry_guard.as_ref().map(|_| sentry_tracing::layer()))
                    .try_init()
                    .ok();
            }
            #[cfg(not(feature = "sentry"))]
            {
                base.try_init().ok();
            }
        }
        _ => {
            let base = tracing_subscriber::registry()
                .with(filter)
                .with(fmt::layer());
            #[cfg(feature = "sentry")]
            {
                base.with(sentry_guard.as_ref().map(|_| sentry_tracing::layer()))
                    .try_init()
                    .ok();
            }
            #[cfg(not(feature = "sentry"))]
            {
                base.try_init().ok();
            }
        }
    }

    #[cfg(feature = "sentry")]
    {
        Ok(match sentry_guard {
            Some(g) => super::sentry::TracingGuard::with_sentry(g),
            None => super::sentry::TracingGuard::new(),
        })
    }
    #[cfg(not(feature = "sentry"))]
    {
        Ok(super::sentry::TracingGuard::new())
    }
}

#[cfg(feature = "sentry")]
fn init_sentry(config: &Config) -> Option<sentry::ClientInitGuard> {
    config
        .sentry
        .as_ref()
        .filter(|sc| !sc.dsn.is_empty())
        .map(|sentry_config| {
            sentry::init((
                sentry_config.dsn.as_str(),
                sentry::ClientOptions {
                    release: sentry::release_name!(),
                    environment: Some(sentry_config.environment.clone().into()),
                    sample_rate: sentry_config.sample_rate,
                    traces_sample_rate: sentry_config.traces_sample_rate,
                    ..Default::default()
                },
            ))
        })
}
