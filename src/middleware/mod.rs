//! HTTP middleware for the modo web framework.
//!
//! This module provides a collection of Tower-compatible middleware layers that
//! cover the most common cross-cutting concerns for HTTP applications:
//!
//! | Function / type | Purpose |
//! |---|---|
//! | [`compression`] | Compress responses (gzip, deflate, brotli, zstd) |
//! | [`request_id`] | Set / propagate `x-request-id` header |
//! | [`catch_panic`] | Convert handler panics into 500 responses |
//! | [`cors`] / [`cors_with`] | CORS headers (static or dynamic origins) |
//! | [`subdomains`] / [`urls`] | CORS origin predicates |
//! | [`csrf`] / [`CsrfConfig`] | Double-submit signed-cookie CSRF protection |
//! | [`error_handler`] | Centralised error-response rendering |
//! | [`security_headers`] / [`SecurityHeadersConfig`] | Security response headers |
//! | [`tracing`] | HTTP request/response lifecycle spans |
//! | [`rate_limit`] / [`rate_limit_with`] | Token-bucket rate limiting |
//! | [`KeyExtractor`] | Trait for custom rate-limit key extraction |
//! | [`PeerIpKeyExtractor`] / [`GlobalKeyExtractor`] | Built-in key extractors |

mod catch_panic;
mod compression;
mod cors;
mod csrf;
mod error_handler;
mod rate_limit;
mod request_id;
mod security_headers;
mod tracing;

pub use self::tracing::tracing;
pub use catch_panic::catch_panic;
pub use compression::compression;
pub use cors::{CorsConfig, cors, cors_with, subdomains, urls};
pub use csrf::{CsrfConfig, CsrfToken, csrf};
pub use error_handler::error_handler;
pub use rate_limit::{
    GlobalKeyExtractor, KeyExtractor, PeerIpKeyExtractor, RateLimitConfig, RateLimitLayer,
    rate_limit, rate_limit_with,
};
pub use request_id::request_id;
pub use security_headers::{SecurityHeadersConfig, security_headers};
