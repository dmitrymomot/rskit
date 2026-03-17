use crate::app::AppState;
use crate::error::{Error, HttpError};
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::{HeaderName, HeaderValue, Request};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use dashmap::DashMap;
use std::sync::Arc;
use std::time::Instant;

use super::ClientIp;

// ---------------------------------------------------------------------------
// Token bucket
// ---------------------------------------------------------------------------

struct TokenBucket {
    tokens: f64,
    last_refill: Instant,
    max_tokens: f64,
    refill_rate: f64, // tokens per second
}

impl TokenBucket {
    fn new(max_tokens: u32, window_secs: u64) -> Self {
        Self {
            tokens: max_tokens as f64,
            last_refill: Instant::now(),
            max_tokens: max_tokens as f64,
            refill_rate: max_tokens as f64 / window_secs as f64,
        }
    }

    fn try_consume(&mut self) -> ConsumeResult {
        self.refill();
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            ConsumeResult::Allowed {
                remaining: self.tokens as u32,
            }
        } else {
            let wait_secs = ((1.0 - self.tokens) / self.refill_rate).ceil() as u64;
            ConsumeResult::Denied {
                retry_after_secs: wait_secs.max(1),
            }
        }
    }

    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        self.tokens = (self.tokens + elapsed * self.refill_rate).min(self.max_tokens);
        self.last_refill = now;
    }

    fn last_active(&self) -> Instant {
        self.last_refill
    }
}

enum ConsumeResult {
    Allowed { remaining: u32 },
    Denied { retry_after_secs: u64 },
}

// ---------------------------------------------------------------------------
// Rate limiter state
// ---------------------------------------------------------------------------

/// Shared token-bucket state for the global rate limiter.
///
/// Keyed by an arbitrary string (IP address, header value, or path).
/// Use `AppBuilder::rate_limit` to configure the global rate limiter,
/// or construct directly for custom middleware.
pub struct RateLimiterState {
    buckets: DashMap<String, TokenBucket>,
    max_tokens: u32,
    window_secs: u64,
}

impl RateLimiterState {
    /// Create a new rate limiter with `max_tokens` per `window_secs` window.
    pub fn new(max_tokens: u32, window_secs: u64) -> Self {
        Self {
            buckets: DashMap::new(),
            max_tokens,
            window_secs,
        }
    }

    fn try_consume(&self, key: &str) -> ConsumeResult {
        let mut entry = self
            .buckets
            .entry(key.to_string())
            .or_insert_with(|| TokenBucket::new(self.max_tokens, self.window_secs));
        entry.value_mut().try_consume()
    }

    fn cleanup(&self, max_age: std::time::Duration) {
        let cutoff = Instant::now() - max_age;
        self.buckets
            .retain(|_, bucket| bucket.last_active() > cutoff);
    }
}

// ---------------------------------------------------------------------------
// Info extractor
// ---------------------------------------------------------------------------

/// Rate limit info injected into request extensions on every request.
#[derive(Debug, Clone)]
pub struct RateLimitInfo {
    pub remaining: u32,
    pub limit: u32,
    pub reset_secs: u64,
}

impl FromRequestParts<AppState> for RateLimitInfo {
    type Rejection = Error;

    async fn from_request_parts(
        parts: &mut Parts,
        _state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<RateLimitInfo>()
            .cloned()
            .ok_or_else(|| Error::internal("rate limit info not found in request extensions"))
    }
}

/// Rate limit info extractor that never rejects.
/// Returns `None` when rate limiting middleware is not configured.
pub struct OptionalRateLimitInfo(pub Option<RateLimitInfo>);

impl std::ops::Deref for OptionalRateLimitInfo {
    type Target = Option<RateLimitInfo>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl FromRequestParts<AppState> for OptionalRateLimitInfo {
    type Rejection = Error;

    async fn from_request_parts(
        parts: &mut Parts,
        _state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        Ok(OptionalRateLimitInfo(
            parts.extensions.get::<RateLimitInfo>().cloned(),
        ))
    }
}

// ---------------------------------------------------------------------------
// Key functions
// ---------------------------------------------------------------------------

/// Function type that extracts a rate-limit key from request parts.
pub type KeyFn = Arc<dyn Fn(&Parts) -> String + Send + Sync>;

/// Key function that uses the resolved client IP address.
pub fn by_ip() -> KeyFn {
    Arc::new(|parts: &Parts| {
        parts
            .extensions
            .get::<ClientIp>()
            .map(|ip| ip.0.to_string())
            .unwrap_or_else(|| "unknown".to_string())
    })
}

/// Key function that uses the value of a named request header.
///
/// # Panics
///
/// Panics at construction time if `name` is not a valid HTTP header name.
pub fn by_header(name: &str) -> KeyFn {
    let name = HeaderName::from_bytes(name.as_bytes())
        .unwrap_or_else(|_| panic!("by_header: invalid HTTP header name {:?}", name));
    Arc::new(move |parts: &Parts| {
        parts
            .headers
            .get(&name)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("unknown")
            .to_string()
    })
}

/// Key function that uses the request path.
pub fn by_path() -> KeyFn {
    Arc::new(|parts: &Parts| parts.uri.path().to_string())
}

// ---------------------------------------------------------------------------
// Middleware factory
// ---------------------------------------------------------------------------

static RATE_LIMIT_HEADER: HeaderName = HeaderName::from_static("x-ratelimit-limit");
static RATE_LIMIT_REMAINING: HeaderName = HeaderName::from_static("x-ratelimit-remaining");
static RATE_LIMIT_RESET: HeaderName = HeaderName::from_static("x-ratelimit-reset");
static RETRY_AFTER: HeaderName = HeaderName::from_static("retry-after");

/// Creates a rate-limiting middleware closure.
///
/// The returned middleware can be applied via `axum::middleware::from_fn`.
/// Use `spawn_cleanup_task` to periodically prune expired buckets.
pub fn rate_limit_middleware(
    limiter: Arc<RateLimiterState>,
    key_fn: KeyFn,
) -> impl Fn(
    Request<axum::body::Body>,
    Next,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Response> + Send>>
+ Clone
+ Send {
    move |request: Request<axum::body::Body>, next: Next| {
        let limiter = limiter.clone();
        let key_fn = key_fn.clone();
        Box::pin(async move {
            let (parts, body) = request.into_parts();
            let key = key_fn(&parts);
            let mut request = Request::from_parts(parts, body);

            let reset_timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
                + limiter.window_secs;

            match limiter.try_consume(&key) {
                ConsumeResult::Allowed { remaining } => {
                    request.extensions_mut().insert(RateLimitInfo {
                        remaining,
                        limit: limiter.max_tokens,
                        reset_secs: reset_timestamp,
                    });

                    let mut response = next.run(request).await;
                    set_rate_headers(
                        response.headers_mut(),
                        limiter.max_tokens,
                        remaining,
                        reset_timestamp,
                    );
                    response
                }
                ConsumeResult::Denied { retry_after_secs } => {
                    let mut response = HttpError::TooManyRequests.into_response();
                    set_rate_headers(
                        response.headers_mut(),
                        limiter.max_tokens,
                        0,
                        reset_timestamp,
                    );
                    if let Ok(v) = HeaderValue::from_str(&retry_after_secs.to_string()) {
                        response.headers_mut().insert(RETRY_AFTER.clone(), v);
                    }
                    response
                }
            }
        })
    }
}

fn set_rate_headers(headers: &mut axum::http::HeaderMap, limit: u32, remaining: u32, reset: u64) {
    if let Ok(v) = HeaderValue::from_str(&limit.to_string()) {
        headers.insert(RATE_LIMIT_HEADER.clone(), v);
    }
    if let Ok(v) = HeaderValue::from_str(&remaining.to_string()) {
        headers.insert(RATE_LIMIT_REMAINING.clone(), v);
    }
    if let Ok(v) = HeaderValue::from_str(&reset.to_string()) {
        headers.insert(RATE_LIMIT_RESET.clone(), v);
    }
}

/// Calculate cleanup interval proportional to the rate limit window.
/// Returns `clamp(window_secs / 2, 1, 60)`.
fn cleanup_interval_secs(window_secs: u64) -> u64 {
    (window_secs / 2).clamp(1, 60)
}

/// Spawns a background task that prunes expired buckets at an interval
/// proportional to the rate-limit window (`clamp(window / 2, 1s, 60s)`).
///
/// Returns the `JoinHandle` so callers can abort it during shutdown.
pub fn spawn_cleanup_task(limiter: Arc<RateLimiterState>) -> tokio::task::JoinHandle<()> {
    let window = limiter.window_secs;
    let interval_secs = cleanup_interval_secs(window);
    tokio::spawn(async move {
        let interval = std::time::Duration::from_secs(interval_secs);
        let max_age = std::time::Duration::from_secs(window * 2);
        loop {
            tokio::time::sleep(interval).await;
            limiter.cleanup(max_age);
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn optional_rate_limit_info_returns_none_without_middleware() {
        use crate::app::{AppState, ServiceRegistry};
        use axum::Router;
        use axum::body::Body;
        use axum::http::{Request, StatusCode};
        use axum::routing::get;
        use tower::ServiceExt;

        let state = AppState {
            services: ServiceRegistry::new(),
            server_config: Default::default(),
            cookie_key: axum_extra::extract::cookie::Key::generate(),
        };

        let app = Router::new()
            .route(
                "/",
                get(|info: OptionalRateLimitInfo| async move {
                    if info.is_none() { "none" } else { "some" }
                }),
            )
            .with_state(state);

        let resp = app
            .oneshot(Request::get("/").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(&body[..], b"none");
    }

    #[test]
    fn test_token_bucket_allows_within_limit() {
        let mut bucket = TokenBucket::new(3, 60);
        assert!(matches!(
            bucket.try_consume(),
            ConsumeResult::Allowed { .. }
        ));
        assert!(matches!(
            bucket.try_consume(),
            ConsumeResult::Allowed { .. }
        ));
        assert!(matches!(
            bucket.try_consume(),
            ConsumeResult::Allowed { .. }
        ));
        assert!(matches!(bucket.try_consume(), ConsumeResult::Denied { .. }));
    }

    #[test]
    fn test_rate_limiter_state_per_key() {
        let limiter = RateLimiterState::new(2, 60);
        assert!(matches!(
            limiter.try_consume("a"),
            ConsumeResult::Allowed { .. }
        ));
        assert!(matches!(
            limiter.try_consume("a"),
            ConsumeResult::Allowed { .. }
        ));
        assert!(matches!(
            limiter.try_consume("a"),
            ConsumeResult::Denied { .. }
        ));
        // Different key should still be allowed
        assert!(matches!(
            limiter.try_consume("b"),
            ConsumeResult::Allowed { .. }
        ));
    }

    #[test]
    fn test_cleanup_removes_old_entries() {
        let limiter = RateLimiterState::new(10, 1);
        limiter.try_consume("old-key");
        assert_eq!(limiter.buckets.len(), 1);
        // Zero max_age means everything is "expired"
        limiter.cleanup(std::time::Duration::ZERO);
        assert_eq!(limiter.buckets.len(), 0);
    }

    #[test]
    fn test_cleanup_interval_calculation() {
        // Small window: 2s -> max(1, 1) capped at 60 = 1s
        assert_eq!(cleanup_interval_secs(2), 1);

        // Medium window: 60s -> max(30, 1) capped at 60 = 30s
        assert_eq!(cleanup_interval_secs(60), 30);

        // Large window: 300s -> max(150, 1) capped at 60 = 60s
        assert_eq!(cleanup_interval_secs(300), 60);

        // Very large window: 3600s -> max(1800, 1) capped at 60 = 60s
        assert_eq!(cleanup_interval_secs(3600), 60);

        // Tiny window: 1s -> max(0, 1) = 1s  (0/2 = 0, clamped to 1)
        assert_eq!(cleanup_interval_secs(1), 1);

        // Zero window (edge case): 0s -> max(0, 1) capped at 60 = 1s
        assert_eq!(cleanup_interval_secs(0), 1);
    }
}
