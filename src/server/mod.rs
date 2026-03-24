//! HTTP server startup and graceful shutdown.
//!
//! This module provides two public items:
//!
//! - [`Config`] — bind address and shutdown timeout, loaded from YAML.
//! - [`http`] — starts a TCP listener and returns an [`HttpServer`] handle
//!   that implements [`crate::runtime::Task`] for use with the [`crate::run!`]
//!   macro.
//!
//! # Example
//!
//! ```no_run
//! use modo::server::{Config, http};
//! use modo::run;
//!
//! #[tokio::main]
//! async fn main() -> modo::Result<()> {
//!     let config = Config::default();
//!     let router = modo::axum::Router::new();
//!     let server = http(router, &config).await?;
//!     run!(server).await
//! }
//! ```

mod config;
mod http;

pub use config::Config;
pub use http::{HttpServer, http};
