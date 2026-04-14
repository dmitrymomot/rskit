use serde::Deserialize;
use tracing_subscriber::prelude::*;
use tracing_subscriber::{EnvFilter, fmt};

/// Configuration for the tracing subscriber.
///
/// Embedded in the top-level `modo::Config` as the `tracing` section:
///
/// ```yaml
/// tracing:
///   level: info
///   format: pretty   # "pretty" | "json" | compact (any other value)
/// ```
///
/// All fields have sane defaults so the entire section can be omitted.
#[non_exhaustive]
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Minimum log level when `RUST_LOG` is not set.
    ///
    /// Accepts any valid [`tracing_subscriber::EnvFilter`] directive such as
    /// `"info"`, `"debug"`, or `"myapp=debug,modo=info"`.
    /// Defaults to `"info"`.
    pub level: String,

    /// Output format: `"pretty"`, `"json"`, or compact (any other value).
    ///
    /// Defaults to `"pretty"`.
    pub format: String,

    /// Sentry error-reporting settings.
    ///
    /// When absent or when the DSN is empty, Sentry is not initialised.
    pub sentry: Option<super::sentry::SentryConfig>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
            format: "pretty".to_string(),
            sentry: None,
        }
    }
}

/// Initialise the global tracing subscriber.
///
/// Reads the log level from `RUST_LOG` if set; falls back to
/// [`Config::level`] otherwise. Selects the output format from
/// [`Config::format`].
///
/// When [`Config::sentry`] contains a non-empty DSN, the Sentry SDK is
/// also initialised and wired to the tracing subscriber via
/// `sentry-tracing`. Sentry support is always compiled in — no feature
/// flag is required.
///
/// Returns a [`TracingGuard`] that must be kept alive for the duration of
/// the process. Dropping it flushes any buffered Sentry events.
///
/// Calling this function more than once in the same process is harmless —
/// subsequent calls attempt `try_init` and silently ignore the
/// "already initialised" error.
///
/// # Errors
///
/// Currently infallible. The `Result` return type is reserved for future
/// validation of the [`Config`] fields at initialisation time.
///
/// [`TracingGuard`]: crate::tracing::TracingGuard
pub fn init(config: &Config) -> crate::error::Result<super::sentry::TracingGuard> {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&config.level));

    let sentry_guard = init_sentry(config);

    match config.format.as_str() {
        "json" => {
            let base = tracing_subscriber::registry()
                .with(filter)
                .with(fmt::layer().json());
            base.with(sentry_guard.as_ref().map(|_| sentry_tracing::layer()))
                .try_init()
                .ok();
        }
        "pretty" => {
            let base = tracing_subscriber::registry()
                .with(filter)
                .with(fmt::layer().pretty());
            base.with(sentry_guard.as_ref().map(|_| sentry_tracing::layer()))
                .try_init()
                .ok();
        }
        _ => {
            let base = tracing_subscriber::registry()
                .with(filter)
                .with(fmt::layer());
            base.with(sentry_guard.as_ref().map(|_| sentry_tracing::layer()))
                .try_init()
                .ok();
        }
    }

    Ok(match sentry_guard {
        Some(g) => super::sentry::TracingGuard::with_sentry(g),
        None => super::sentry::TracingGuard::new(),
    })
}

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
