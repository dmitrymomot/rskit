use axum::body::Body;
use governor::middleware::StateInformationMiddleware;
use http::{Response, StatusCode};
use serde::Deserialize;
use tower_governor::governor::GovernorConfigBuilder;
use tower_governor::key_extractor::{KeyExtractor, PeerIpKeyExtractor};
use tower_governor::{GovernorError, GovernorLayer};

/// Configuration for the rate-limiting middleware.
///
/// Uses a token-bucket algorithm (via `tower_governor`). Each unique key
/// (typically the client IP) gets `burst_size` tokens; one token is replenished
/// every `1 / per_second` seconds. When tokens are exhausted the request
/// receives a `429 Too Many Requests` response.
///
/// Rate-limit response headers (`x-ratelimit-limit`, `x-ratelimit-remaining`,
/// etc.) are always included. The `use_headers` field is reserved for future
/// use and currently has no effect.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct RateLimitConfig {
    /// Token replenish rate (tokens per second).
    pub per_second: u64,
    /// Maximum number of tokens (requests) allowed in a burst.
    pub burst_size: u32,
    /// Whether to include `x-ratelimit-*` headers in responses.
    ///
    /// Headers are always enabled in the current implementation because
    /// `tower_governor` encodes this choice at the type level. This field
    /// exists so that configuration files can express intent; a future
    /// version may honour it.
    pub use_headers: bool,
    /// How often (in seconds) to purge expired entries from the rate-limit map.
    pub cleanup_interval_secs: u64,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            per_second: 1,
            burst_size: 10,
            use_headers: true,
            cleanup_interval_secs: 60,
        }
    }
}

/// Returns a rate-limiting layer keyed by peer IP address.
///
/// Suitable for production use where each client is identified by its
/// socket address. Requires the server to be started with
/// `into_make_service_with_connect_info::<SocketAddr>()` so that
/// `ConnectInfo<SocketAddr>` is available in request extensions.
pub fn rate_limit(
    config: &RateLimitConfig,
) -> GovernorLayer<PeerIpKeyExtractor, StateInformationMiddleware, Body> {
    rate_limit_with(config, PeerIpKeyExtractor)
}

/// Returns a rate-limiting layer with a custom key extractor.
///
/// Use this when the default IP-based extraction is not appropriate — for
/// example, rate-limiting by API key, user ID, or using
/// [`tower_governor::key_extractor::GlobalKeyExtractor`] for a single
/// shared bucket.
pub fn rate_limit_with<K>(
    config: &RateLimitConfig,
    extractor: K,
) -> GovernorLayer<K, StateInformationMiddleware, Body>
where
    K: KeyExtractor,
    K::Key: Send + Sync + 'static,
{
    let governor_config = GovernorConfigBuilder::default()
        .key_extractor(extractor)
        .per_second(config.per_second)
        .burst_size(config.burst_size)
        .use_headers()
        .finish()
        .expect("valid rate-limit configuration");

    // Spawn a background task to periodically purge expired entries.
    let interval = std::time::Duration::from_secs(config.cleanup_interval_secs);
    let limiter = governor_config.limiter().clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(interval).await;
            limiter.retain_recent();
        }
    });

    GovernorLayer::new(governor_config).error_handler(error_handler)
}

/// Converts a [`GovernorError`] into an HTTP response with a `modo::Error`
/// stored in extensions (consistent with other modo middleware).
fn error_handler(error: GovernorError) -> Response<Body> {
    match error {
        GovernorError::TooManyRequests { wait_time, headers } => {
            let modo_error =
                crate::error::Error::too_many_requests(format!("retry after {wait_time}s"));
            let mut response = Response::new(Body::from(format!(
                r#"{{"error":{{"status":429,"message":"too many requests","retry_after":{wait_time}}}}}"#
            )));
            *response.status_mut() = StatusCode::TOO_MANY_REQUESTS;
            if let Some(h) = headers {
                response.headers_mut().extend(h);
            }
            response.extensions_mut().insert(modo_error);
            response
        }
        GovernorError::UnableToExtractKey => {
            let modo_error = crate::error::Error::internal("unable to extract rate-limit key");
            let mut response = Response::new(Body::from(
                r#"{"error":{"status":500,"message":"unable to extract rate-limit key"}}"#,
            ));
            *response.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
            response.extensions_mut().insert(modo_error);
            response
        }
        GovernorError::Other { code, msg, headers } => {
            let message =
                msg.unwrap_or_else(|| code.canonical_reason().unwrap_or("error").to_string());
            let modo_error = crate::error::Error::new(code, &message);
            let mut response = Response::new(Body::from(format!(
                r#"{{"error":{{"status":{},"message":"{}"}}}}"#,
                code.as_u16(),
                message
            )));
            *response.status_mut() = code;
            if let Some(h) = headers {
                response.headers_mut().extend(h);
            }
            response.extensions_mut().insert(modo_error);
            response
        }
    }
}
