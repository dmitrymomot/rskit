//! # modo::tracing
//!
//! Tracing initialisation and structured logging for modo applications.
//!
//! Configures [`tracing_subscriber`] at application startup. Call [`init`]
//! once from `main` — typically before starting the HTTP server — and hold
//! the returned [`TracingGuard`] for the lifetime of the process.
//!
//! ## Provides
//!
//! | Item | Description |
//! |------|-------------|
//! | [`Config`] | Log level, output format, and optional Sentry settings |
//! | [`init`] | Initialises the global tracing subscriber; returns [`TracingGuard`] |
//! | [`TracingGuard`] | RAII guard that keeps the subscriber (and Sentry client) alive |
//! | [`SentryConfig`] | Sentry DSN, environment, and sampling rates (requires `sentry` feature) |
//! | `info!`, `debug!`, `warn!`, `error!`, `trace!` | Re-exported tracing macros |
//!
//! ## Log format
//!
//! The format is selected by [`Config::format`]:
//!
//! | Value | Description |
//! |-------|-------------|
//! | `"pretty"` (default) | Human-readable multi-line output |
//! | `"json"` | Machine-readable JSON, one object per line |
//! | anything else | Compact single-line output |
//!
//! The active log level is read from the `RUST_LOG` environment variable
//! when present; otherwise [`Config::level`] is used.
//!
//! ## Sentry integration
//!
//! When compiled with the `sentry` feature, populate [`SentryConfig`]
//! inside [`Config::sentry`] to enable error and performance reporting.
//! The Sentry SDK is initialised inside [`init`] and flushed when the
//! [`TracingGuard`] is shut down.
//!
//! ## Quick start
//!
//! ```rust,no_run
//! use modo::config::load;
//! use modo::Config;
//! use modo::runtime::Task;
//!
//! #[tokio::main]
//! async fn main() -> modo::Result<()> {
//!     let config: Config = load("config/").unwrap();
//!     let guard = modo::tracing::init(&config.tracing)?;
//!
//!     // ... start server, then on shutdown:
//!     guard.shutdown().await
//! }
//! ```

mod init;
mod sentry;

pub use init::{Config, init};
pub use sentry::SentryConfig;
pub use sentry::TracingGuard;

// Re-export tracing macros so $crate::tracing::info! works in run! macro
pub use ::tracing::{debug, error, info, trace, warn};
