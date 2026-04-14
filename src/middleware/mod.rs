//! # modo::middleware
//!
//! Universal HTTP middleware for the modo web framework.
//!
//! Provides a collection of Tower-compatible middleware layers covering
//! the most common cross-cutting concerns — compression, request IDs,
//! panic recovery, CORS, CSRF, centralised error rendering, security
//! headers, request tracing, and rate limiting. Always available (no
//! feature flag required).
//!
//! ## Relationship to `modo::middlewares`
//!
//! This module ships the framework-universal layers. The virtual
//! [`modo::middlewares`](crate::middlewares) module is a flat
//! wiring-site index that re-exports **both** these universal
//! middlewares **and** domain-specific layers from feature-gated
//! modules (e.g. `session`, `tenant`, `auth`, `flash`, `ip`, `tier`,
//! `geolocation`, `template`). Reach for `modo::middlewares` when you
//! want a single namespace at your `.layer(...)` call sites; reach for
//! `modo::middleware` when you only need the universal layers or the
//! supporting configuration and extractor types.
//!
//! ## Provided items
//!
//! | Function / type | Purpose |
//! |---|---|
//! | [`compression`] | Compress responses (gzip, deflate, brotli, zstd) |
//! | [`request_id`] | Set / propagate `x-request-id` header |
//! | [`catch_panic`] | Convert handler panics into 500 responses |
//! | [`cors`] / [`cors_with`] | CORS headers (static or dynamic origins) |
//! | [`CorsConfig`] | CORS configuration |
//! | [`subdomains`] / [`urls`] | CORS origin predicates |
//! | [`csrf`] / [`CsrfConfig`] | Double-submit signed-cookie CSRF protection |
//! | [`CsrfToken`] | CSRF token in request/response extensions |
//! | [`error_handler`] | Centralised error-response rendering |
//! | [`security_headers`] / [`SecurityHeadersConfig`] | Security response headers |
//! | [`tracing`] | HTTP request/response lifecycle spans |
//! | [`rate_limit`] / [`rate_limit_with`] | Token-bucket rate limiting |
//! | [`RateLimitConfig`] | Rate-limit configuration |
//! | [`RateLimitLayer`] | Tower layer produced by `rate_limit` / `rate_limit_with` |
//! | [`KeyExtractor`] | Trait for custom rate-limit key extraction |
//! | [`PeerIpKeyExtractor`] / [`GlobalKeyExtractor`] | Built-in key extractors |
//!
//! ## Quick start
//!
//! ```rust,no_run
//! use axum::{Router, routing::get};
//! use axum::response::IntoResponse;
//! use modo::middleware::*;
//!
//! async fn render_error(err: modo::Error, _parts: http::request::Parts) -> axum::response::Response {
//!     (err.status(), err.message().to_string()).into_response()
//! }
//!
//! let app: Router = Router::new()
//!     .route("/", get(|| async { "hello" }))
//!     .layer(error_handler(render_error))
//!     .layer(compression())
//!     .layer(request_id())
//!     .layer(catch_panic())
//!     .layer(tracing());
//! ```

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
