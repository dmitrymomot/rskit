//! # modo
//!
//! A Rust web framework for small monolithic apps. Single crate, zero proc
//! macros, built on [`axum`] 0.8 with [`libsql`](https://crates.io/crates/libsql)
//! (SQLite) for persistence. Handlers are plain `async fn`, routes use
//! [`axum::Router`] directly, services are wired explicitly in `main()`, and
//! database queries use raw libsql.
//!
//! ## Quick start
//!
//! Add to `Cargo.toml`:
//!
//! ```toml
//! [dependencies]
//! modo = { package = "modo-rs", version = "0.10" }
//!
//! [dev-dependencies]
//! modo = { package = "modo-rs", version = "0.10", features = ["test-helpers"] }
//! ```
//!
//! Minimal application:
//!
//! ```rust,no_run
//! use modo::{Config, Result};
//! use modo::axum::{Router, routing::get};
//!
//! async fn hello() -> &'static str { "Hello, modo!" }
//!
//! #[tokio::main]
//! async fn main() -> Result<()> {
//!     let config: Config = modo::config::load("config/")?;
//!     let app = Router::new().route("/", get(hello));
//!     let server = modo::server::http(app, &config.server).await?;
//!     modo::run!(server).await
//! }
//! ```
//!
//! Inside a handler module, pull in the common handler-time types with:
//!
//! ```ignore
//! use modo::prelude::*;
//! ```
//!
//! ## Key crate-level exports
//!
//! These items are re-exported at the crate root for convenience:
//!
//! - [`Error`] — the framework error type (HTTP status + message + optional source/code)
//! - [`Result`] — `std::result::Result<T, Error>` alias
//! - [`Config`] — top-level application configuration
//! - [`run!`](crate::run) — macro that waits for SIGTERM/SIGINT then shuts down each
//!   supplied [`Task`](crate::runtime::Task) in declaration order
//!
//! ## Virtual flat-index modules
//!
//! Three virtual modules re-export items across the crate so you don't have
//! to remember which source module they live in:
//!
//! - [`middlewares`] — every public middleware constructor
//! - [`extractors`] — every public request extractor
//! - [`guards`] — every route-level gating layer applied via `.route_layer()`
//!
//! [`prelude`] bundles the extras a typical handler signature needs on top of
//! those (`Error`, `Result`, `AppState`, `Session`, `Role`, `Flash`, `ClientIp`,
//! `Tenant`, `TenantId`, `I18n`, `Translator`, and the `Validate` trio).
//!
//! ## Features
//!
//! Every module is always compiled — no cargo features gate production code.
//! The only feature flag is `test-helpers`, which exposes in-memory backends
//! and test harnesses ([`testing`]); enable it in your `[dev-dependencies]`.
//!
//! | Feature | Purpose |
//! |---------|---------|
//! | `test-helpers` | Enables [`testing`] module with `TestDb`, `TestApp`, `TestSession`, and all in-memory/stub backends |
//!
//! ## Dependency re-exports
//!
//! modo re-exports the four crates that appear in nearly every handler
//! signature, so you don't need to pin matching versions yourself:
//!
//! - [`axum`] — router, extractors, responses
//! - [`serde`] — `Serialize` / `Deserialize` derives
//! - [`serde_json`] — JSON values and macros
//! - [`tokio`] — runtime, tasks, sync primitives

pub mod config;
pub mod error;
pub mod runtime;
pub mod server;
pub mod service;

pub mod cache;
pub mod db;
pub mod storage;

pub mod cookie;
pub mod extractor;
pub mod flash;
pub mod ip;
pub mod middleware;
pub mod sse;

pub mod auth;
pub mod tenant;
pub mod tier;

pub mod cron;
pub mod job;

pub mod email;
pub mod i18n;
pub mod qrcode;
pub mod template;
pub mod webhook;

pub mod audit;
pub mod health;
pub mod tracing;

pub mod dns;
pub mod embed;
pub mod geolocation;

pub mod encoding;
pub mod id;
pub mod sanitize;
pub mod validate;

#[cfg(feature = "test-helpers")]
pub mod testing;

pub mod extractors;
pub mod guards;
pub mod middlewares;
pub mod prelude;

pub use config::Config;
pub use error::{Error, Result};

pub use axum;
pub use serde;
pub use serde_json;
pub use tokio;
