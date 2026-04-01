//! HTTP server startup and graceful shutdown.
//!
//! This module provides:
//!
//! - [`Config`] — bind address and shutdown timeout, loaded from YAML.
//! - [`http`] — starts a TCP listener and returns an [`HttpServer`] handle.
//! - [`HttpServer`] — opaque server handle that implements
//!   [`crate::runtime::Task`] for use with the [`crate::run!`] macro.
//!
//! # Example
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
pub use http::{HttpServer, http};
