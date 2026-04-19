//! # modo::server
//!
//! HTTP server startup, host-based routing, and graceful shutdown.
//!
//! Provides:
//!
//! - [`Config`] — bind address and shutdown timeout, deserialized from the
//!   `server` YAML section.
//! - [`http()`] — binds a TCP listener, spawns the server on a background task,
//!   and returns an [`HttpServer`] handle.
//! - [`HttpServer`] — opaque server handle that implements
//!   [`crate::runtime::Task`] so it composes with the [`crate::run!`] macro for
//!   coordinated graceful shutdown.
//! - [`HostRouter`] — routes requests to different axum routers by `Host` header;
//!   supports exact matches and single-level wildcard subdomains with an
//!   optional fallback. Implements `Into<axum::Router>` so it plugs directly
//!   into [`http()`].
//! - [`MatchedHost`] — axum extractor that exposes the subdomain captured by a
//!   wildcard `HostRouter` pattern (plus the pattern itself).
//!
//! Trailing slashes are stripped from request paths before routing, so `/app`
//! and `/app/` resolve to the same handler (the root `/` is preserved).
//!
//! ## Quick start
//!
//! ```rust,no_run
//! use modo::{Config, Result};
//! use modo::axum::Router;
//!
//! #[tokio::main]
//! async fn main() -> Result<()> {
//!     let config: Config = modo::config::load("config/")?;
//!     let app = Router::new();
//!     let server = modo::server::http(app, &config.server).await?;
//!     modo::run!(server).await
//! }
//! ```

mod config;
mod host_router;
mod http;

pub use config::Config;
pub use host_router::{HostRouter, MatchedHost};
pub use http::{HttpServer, http};
