use std::collections::HashMap;
use std::future::Future;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::RwLock;
use std::task::{Context, Poll};
use std::time::Instant;

use axum::body::Body;
use http::{Request, Response, StatusCode};
use serde::Deserialize;
use tokio_util::sync::CancellationToken;
use tower::{Layer, Service};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configuration for the rate-limiting middleware.
///
/// Uses a token-bucket algorithm. Each unique key (typically the client IP)
/// gets `burst_size` tokens; one token is replenished every `1 / per_second`
/// seconds. When tokens are exhausted the request receives a
/// `429 Too Many Requests` response.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct RateLimitConfig {
    /// Token replenish rate (tokens per second).
    pub per_second: u64,
    /// Maximum number of tokens (requests) allowed in a burst.
    pub burst_size: u32,
    /// Whether to include `x-ratelimit-*` headers in responses.
    pub use_headers: bool,
    /// How often (in seconds) to purge expired entries from the rate-limit map.
    pub cleanup_interval_secs: u64,
    /// Maximum number of tracked keys. New keys are rejected when the limit
    /// is reached. Set to `0` to disable the cap.
    pub max_keys: usize,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            per_second: 1,
            burst_size: 10,
            use_headers: true,
            cleanup_interval_secs: 60,
            max_keys: 10_000,
        }
    }
}

// ---------------------------------------------------------------------------
// Token bucket
// ---------------------------------------------------------------------------

struct TokenBucket {
    tokens: f64,
    last_refill: Instant,
}

enum CheckResult {
    Allowed { remaining: u32 },
    Rejected { retry_after_secs: f64 },
}

impl TokenBucket {
    fn new(burst_size: u32) -> Self {
        Self {
            tokens: burst_size as f64,
            last_refill: Instant::now(),
        }
    }

    fn check(&mut self, per_second: u64, burst_size: u32) -> CheckResult {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        self.last_refill = now;

        // Refill tokens
        self.tokens = (self.tokens + elapsed * per_second as f64).min(burst_size as f64);

        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            CheckResult::Allowed {
                remaining: self.tokens as u32,
            }
        } else {
            let deficit = 1.0 - self.tokens;
            let wait = deficit / per_second as f64;
            CheckResult::Rejected {
                retry_after_secs: wait,
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Sharded map
// ---------------------------------------------------------------------------

const DEFAULT_SHARDS: usize = 16;

struct ShardedMap {
    shards: Vec<RwLock<HashMap<String, TokenBucket>>>,
}

impl ShardedMap {
    fn new(num_shards: usize) -> Self {
        let mut shards = Vec::with_capacity(num_shards);
        for _ in 0..num_shards {
            shards.push(RwLock::new(HashMap::new()));
        }
        Self { shards }
    }

    fn shard_index(&self, key: &str) -> usize {
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        hasher.finish() as usize % self.shards.len()
    }

    fn check_or_insert(
        &self,
        key: &str,
        per_second: u64,
        burst_size: u32,
        max_keys: usize,
    ) -> CheckResult {
        let idx = self.shard_index(key);
        let shard = &self.shards[idx];

        // Try read lock first — fast path for existing keys
        {
            let read = shard.read().unwrap();
            if read.contains_key(key) {
                drop(read);
                // Need write lock to mutate the bucket
                let mut write = shard.write().unwrap();
                if let Some(bucket) = write.get_mut(key) {
                    return bucket.check(per_second, burst_size);
                }
            }
        }

        // Check total keys BEFORE acquiring the write lock
        if max_keys > 0 {
            let total: usize = self.shards.iter().map(|s| s.read().unwrap().len()).sum();
            if total >= max_keys {
                return CheckResult::Rejected {
                    retry_after_secs: 1.0,
                };
            }
        }

        // Write lock — insert new key
        let mut write = shard.write().unwrap();
        // Re-check after acquiring write lock (race condition)
        if let Some(bucket) = write.get_mut(key) {
            return bucket.check(per_second, burst_size);
        }

        let mut bucket = TokenBucket::new(burst_size);
        let result = bucket.check(per_second, burst_size);
        write.insert(key.to_string(), bucket);
        result
    }

    fn cleanup(&self, per_second: u64, burst_size: u32) {
        let max_idle = if per_second > 0 {
            std::time::Duration::from_secs_f64(burst_size as f64 / per_second as f64)
        } else {
            std::time::Duration::from_secs(3600)
        };
        let now = Instant::now();

        for shard in &self.shards {
            let mut write = shard.write().unwrap();
            write.retain(|_, bucket| now.duration_since(bucket.last_refill) < max_idle);
        }
    }
}

// ---------------------------------------------------------------------------
// Key extraction
// ---------------------------------------------------------------------------

/// Trait for extracting a rate-limit key from an incoming request.
///
/// Implementations should return `Some(key)` when a key can be determined
/// (e.g. from the peer IP or an API key header) and `None` when the key
/// cannot be extracted — in which case the middleware returns a 500 error.
pub trait KeyExtractor: Clone + Send + Sync + 'static {
    fn extract<B>(&self, req: &Request<B>) -> Option<String>;
}

/// Extracts the rate-limit key from the peer IP address.
///
/// Requires the server to be started with
/// `into_make_service_with_connect_info::<SocketAddr>()` so that
/// `ConnectInfo<SocketAddr>` is available in request extensions.
#[derive(Debug, Clone)]
pub struct PeerIpKeyExtractor;

impl KeyExtractor for PeerIpKeyExtractor {
    fn extract<B>(&self, req: &Request<B>) -> Option<String> {
        req.extensions()
            .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
            .map(|ci| ci.0.ip().to_string())
    }
}

/// A key extractor that uses a single shared bucket for all requests.
///
/// Useful for applying a global rate limit regardless of the client.
#[derive(Debug, Clone)]
pub struct GlobalKeyExtractor;

impl KeyExtractor for GlobalKeyExtractor {
    fn extract<B>(&self, _req: &Request<B>) -> Option<String> {
        Some("__global__".to_string())
    }
}

// ---------------------------------------------------------------------------
// Tower Layer + Service
// ---------------------------------------------------------------------------

/// A [`tower::Layer`] that applies rate limiting to all requests.
pub struct RateLimitLayer<K> {
    state: Arc<ShardedMap>,
    config: RateLimitConfig,
    extractor: K,
}

impl<K: Clone> Clone for RateLimitLayer<K> {
    fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
            config: self.config.clone(),
            extractor: self.extractor.clone(),
        }
    }
}

impl<S, K: KeyExtractor> Layer<S> for RateLimitLayer<K> {
    type Service = RateLimitService<S, K>;

    fn layer(&self, inner: S) -> Self::Service {
        RateLimitService {
            inner,
            state: self.state.clone(),
            config: self.config.clone(),
            extractor: self.extractor.clone(),
        }
    }
}

/// The [`tower::Service`] created by [`RateLimitLayer`].
pub struct RateLimitService<S, K> {
    inner: S,
    state: Arc<ShardedMap>,
    config: RateLimitConfig,
    extractor: K,
}

impl<S: Clone, K: Clone> Clone for RateLimitService<S, K> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            state: self.state.clone(),
            config: self.config.clone(),
            extractor: self.extractor.clone(),
        }
    }
}

impl<S, K> Service<Request<Body>> for RateLimitService<S, K>
where
    S: Service<Request<Body>, Response = Response<Body>> + Clone + Send + 'static,
    S::Future: Send,
    K: KeyExtractor,
{
    type Response = Response<Body>;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        let Some(key) = self.extractor.extract(&req) else {
            // Cannot extract key — return 500
            let modo_error = crate::error::Error::internal("unable to extract rate-limit key");
            let mut response = Response::new(Body::from(
                r#"{"error":{"status":500,"message":"unable to extract rate-limit key"}}"#,
            ));
            *response.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
            response.extensions_mut().insert(modo_error);
            return Box::pin(async move { Ok(response) });
        };

        let result = self.state.check_or_insert(
            &key,
            self.config.per_second,
            self.config.burst_size,
            self.config.max_keys,
        );

        match result {
            CheckResult::Rejected { retry_after_secs } => {
                let retry_secs = retry_after_secs.ceil() as u64;
                let modo_error =
                    crate::error::Error::too_many_requests(format!("retry after {retry_secs}s"));
                let mut response = Response::new(Body::from(format!(
                    r#"{{"error":{{"status":429,"message":"too many requests","retry_after":{retry_secs}}}}}"#
                )));
                *response.status_mut() = StatusCode::TOO_MANY_REQUESTS;

                if self.config.use_headers {
                    let headers = response.headers_mut();
                    headers.insert("retry-after", retry_secs.into());
                    headers.insert("x-ratelimit-limit", self.config.burst_size.into());
                    headers.insert("x-ratelimit-remaining", 0u32.into());
                }

                response.extensions_mut().insert(modo_error);
                Box::pin(async move { Ok(response) })
            }
            CheckResult::Allowed { remaining } => {
                let use_headers = self.config.use_headers;
                let burst_size = self.config.burst_size;
                let per_second = self.config.per_second;
                let mut inner = self.inner.clone();

                Box::pin(async move {
                    let mut response = inner.call(req).await?;

                    if use_headers {
                        let headers = response.headers_mut();
                        if !headers.contains_key("x-ratelimit-limit") {
                            headers.insert("x-ratelimit-limit", burst_size.into());
                        }
                        if !headers.contains_key("x-ratelimit-remaining") {
                            headers.insert("x-ratelimit-remaining", remaining.into());
                        }
                        if !headers.contains_key("x-ratelimit-reset") {
                            let reset_secs = if per_second > 0 {
                                let now = std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap()
                                    .as_secs();
                                now + (burst_size as u64 / per_second)
                            } else {
                                0
                            };
                            headers.insert("x-ratelimit-reset", reset_secs.into());
                        }
                    }

                    Ok(response)
                })
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Public constructor functions
// ---------------------------------------------------------------------------

/// Returns a rate-limiting layer keyed by peer IP address.
///
/// Suitable for production use where each client is identified by its
/// socket address. Requires the server to be started with
/// `into_make_service_with_connect_info::<SocketAddr>()` so that
/// `ConnectInfo<SocketAddr>` is available in request extensions.
///
/// A background task is spawned to periodically clean up expired entries;
/// it is cancelled when the given [`CancellationToken`] is cancelled.
pub fn rate_limit(
    config: &RateLimitConfig,
    cancel: CancellationToken,
) -> RateLimitLayer<PeerIpKeyExtractor> {
    rate_limit_with(config, PeerIpKeyExtractor, cancel)
}

/// Returns a rate-limiting layer with a custom key extractor.
///
/// Use this when the default IP-based extraction is not appropriate — for
/// example, rate-limiting by API key, user ID, or using
/// [`GlobalKeyExtractor`] for a single shared bucket.
///
/// A background task is spawned to periodically clean up expired entries;
/// it is cancelled when the given [`CancellationToken`] is cancelled.
pub fn rate_limit_with<K: KeyExtractor>(
    config: &RateLimitConfig,
    extractor: K,
    cancel: CancellationToken,
) -> RateLimitLayer<K> {
    let state = Arc::new(ShardedMap::new(DEFAULT_SHARDS));
    let cleanup_state = state.clone();
    let per_second = config.per_second;
    let burst_size = config.burst_size;
    let interval = std::time::Duration::from_secs(config.cleanup_interval_secs);

    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                _ = tokio::time::sleep(interval) => {
                    cleanup_state.cleanup(per_second, burst_size);
                }
            }
        }
    });

    RateLimitLayer {
        state,
        config: config.clone(),
        extractor,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- TokenBucket tests --

    #[test]
    fn token_bucket_allows_within_burst() {
        let mut bucket = TokenBucket::new(3);
        for _ in 0..3 {
            assert!(matches!(bucket.check(1, 3), CheckResult::Allowed { .. }));
        }
    }

    #[test]
    fn token_bucket_rejects_over_burst() {
        let mut bucket = TokenBucket::new(2);
        bucket.check(1, 2); // 1
        bucket.check(1, 2); // 2
        assert!(matches!(bucket.check(1, 2), CheckResult::Rejected { .. }));
    }

    #[test]
    fn token_bucket_refills_over_time() {
        let mut bucket = TokenBucket::new(1);
        bucket.check(10, 1); // exhaust
        // Manually set last_refill to 1 second ago
        bucket.last_refill = Instant::now() - std::time::Duration::from_secs(1);
        assert!(matches!(bucket.check(10, 1), CheckResult::Allowed { .. }));
    }

    #[test]
    fn token_bucket_remaining_count() {
        let mut bucket = TokenBucket::new(5);
        match bucket.check(1, 5) {
            CheckResult::Allowed { remaining } => assert_eq!(remaining, 4),
            _ => panic!("expected Allowed"),
        }
    }

    #[test]
    fn token_bucket_retry_after_positive() {
        let mut bucket = TokenBucket::new(1);
        bucket.check(1, 1); // exhaust
        match bucket.check(1, 1) {
            CheckResult::Rejected { retry_after_secs } => {
                assert!(retry_after_secs > 0.0);
            }
            _ => panic!("expected Rejected"),
        }
    }

    // -- ShardedMap tests --

    #[test]
    fn sharded_map_allows_new_key() {
        let map = ShardedMap::new(4);
        assert!(matches!(
            map.check_or_insert("ip1", 1, 5, 100),
            CheckResult::Allowed { .. }
        ));
    }

    #[test]
    fn sharded_map_tracks_per_key() {
        let map = ShardedMap::new(4);
        // Exhaust key "a" (burst 1)
        map.check_or_insert("a", 1, 1, 100);
        assert!(matches!(
            map.check_or_insert("a", 1, 1, 100),
            CheckResult::Rejected { .. }
        ));
        // Key "b" should still be allowed
        assert!(matches!(
            map.check_or_insert("b", 1, 1, 100),
            CheckResult::Allowed { .. }
        ));
    }

    #[test]
    fn sharded_map_max_keys_rejects_new() {
        let map = ShardedMap::new(2);
        map.check_or_insert("a", 1, 5, 2);
        map.check_or_insert("b", 1, 5, 2);
        // Third key should be rejected (max_keys = 2)
        assert!(matches!(
            map.check_or_insert("c", 1, 5, 2),
            CheckResult::Rejected { .. }
        ));
    }

    #[test]
    fn sharded_map_cleanup_removes_stale() {
        let map = ShardedMap::new(2);
        map.check_or_insert("a", 1, 1, 100);
        // Manually age the entry
        {
            let mut shard = map.shards[map.shard_index("a")].write().unwrap();
            if let Some(bucket) = shard.get_mut("a") {
                bucket.last_refill = Instant::now() - std::time::Duration::from_secs(10);
            }
        }
        map.cleanup(1, 1); // max_idle = 1s, entry is 10s old
        // Entry should be gone — next check creates a fresh bucket
        assert!(matches!(
            map.check_or_insert("a", 1, 1, 100),
            CheckResult::Allowed { .. }
        ));
    }
}
