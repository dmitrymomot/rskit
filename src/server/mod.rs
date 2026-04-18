//! # modo::server
//!
//! HTTP server startup, host-based routing, and graceful shutdown.
//!
//! Provides:
//!
//! - [`Config`] — bind address and shutdown timeout, loaded from YAML.
//! - [`http()`] — binds a TCP listener and returns an [`HttpServer`] handle.
//! - [`HttpServer`] — opaque server handle that implements
//!   [`crate::runtime::Task`] for use with the [`crate::run!`] macro.
//! - [`HostRouter`] — routes requests to different axum routers by `Host` header;
//!   supports exact matches and single-level wildcard subdomains.
//! - [`MatchedHost`] — axum extractor for the subdomain captured by a wildcard
//!   `HostRouter` pattern.
//!
//! Trailing slashes are stripped from request paths before routing, so `/app`
//! and `/app/` resolve to the same handler (the root `/` is preserved).
//!
//! ## Quick start
//!
//! ```no_run
//! use modo::server::{Config, http};
//!
//! #[tokio::main]
//! async fn main() -> modo::Result<()> {
//!     let config = Config::default();
//!     let router = modo::axum::Router::new();
//!     let server = http(router, &config).await?;
//!     modo::run!(server).await
//! }
//! ```

mod config;
mod host_router;
mod http;

pub use config::Config;
pub use host_router::{HostRouter, MatchedHost};
pub use http::{HttpServer, http};
