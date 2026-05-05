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
//! | [`SentryConfig`] | Sentry DSN, environment, and sampling rates |
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
//! Sentry support is always compiled in. Populate [`SentryConfig`] inside
//! [`Config::sentry`] with a non-empty DSN to enable error and performance
//! reporting at runtime. The Sentry SDK is initialised inside [`init`] and
//! flushed when the [`TracingGuard`] is shut down. When the DSN is empty
//! or the `sentry` section is omitted, Sentry is silently skipped.
//!
//! ## HTTP request spans
//!
//! This module only handles *subscriber* setup. HTTP request/response
//! spans are produced by [`crate::middleware::tracing`], which wires a
//! `tower_http::trace::TraceLayer` with [`crate::middleware::ModoMakeSpan`].
//! `ModoMakeSpan` pre-declares a `tenant_id = tracing::field::Empty`
//! field so the tenant middleware can later fill it via
//! `span.record("tenant_id", ...)`. `tracing` silently drops `record()`
//! calls for fields that were not declared at span construction — any
//! new field a middleware needs to fill must therefore be added to
//! `ModoMakeSpan` first. All tracing field names use snake_case
//! (`user_id`, `session_id`, `job_id`, etc.).
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
