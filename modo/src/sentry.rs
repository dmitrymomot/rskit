use axum::http::Request;
use axum::middleware::Next;
use axum::response::Response;
use serde::Deserialize;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

/// Sentry error-tracking configuration.
///
/// Deserialized from the `sentry` key in YAML config.
/// Sentry is enabled when `dsn` is non-empty.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct SentryConfig {
    /// Sentry DSN. Empty string disables Sentry.
    pub dsn: String,
    /// Environment tag sent to Sentry (e.g. "production", "development").
    pub environment: String,
    /// Fraction of transactions to send for performance monitoring (0.0–1.0).
    pub traces_sample_rate: f32,
}

impl Default for SentryConfig {
    fn default() -> Self {
        Self {
            dsn: String::new(),
            environment: "development".to_string(),
            traces_sample_rate: 0.0,
        }
    }
}

/// Trait for extracting Sentry config from any application config type.
///
/// `AppConfig` implements this automatically. Custom config types must
/// implement this trait — the default returns `None` (Sentry disabled).
pub trait SentryConfigProvider {
    fn sentry_config(&self) -> Option<&SentryConfig> {
        None
    }
}

/// Initialize the tracing subscriber with stdout (always) and Sentry (if configured).
///
/// Returns the Sentry `ClientInitGuard` which must be held alive for the
/// lifetime of the application. Dropping it flushes and disables Sentry.
pub fn init_tracing(sentry_cfg: Option<&SentryConfig>) -> Option<sentry::ClientInitGuard> {
    let guard = sentry_cfg.filter(|s| !s.dsn.is_empty()).map(|cfg| {
        sentry::init(sentry::ClientOptions {
            dsn: cfg.dsn.parse().ok(),
            release: sentry::release_name!(),
            environment: Some(cfg.environment.clone().into()),
            traces_sample_rate: cfg.traces_sample_rate,
            ..Default::default()
        })
    });

    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,sqlx::query=warn"));

    if guard.is_some() {
        tracing_subscriber::registry()
            .with(env_filter)
            .with(tracing_subscriber::fmt::layer())
            .with(sentry::integrations::tracing::layer())
            .init();
    } else {
        tracing_subscriber::fmt().with_env_filter(env_filter).init();
    }

    guard
}

/// Middleware that tags the current Sentry scope with the request ID.
pub async fn sentry_request_id_middleware(
    request: Request<axum::body::Body>,
    next: Next,
) -> Response {
    if let Some(rid) = request.extensions().get::<crate::request_id::RequestId>() {
        sentry::configure_scope(|scope| {
            scope.set_tag("request_id", rid.as_str());
        });
    }
    next.run(request).await
}
