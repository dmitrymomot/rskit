//! # modo
//!
//! A Rust web framework for small monolithic apps. Single crate, zero proc
//! macros, built on [`axum`] 0.8 with [`libsql`](https://crates.io/crates/libsql)
//! (SQLite) for persistence. Handlers are plain `async fn`, routes use
//! [`axum::Router`] directly, services are wired explicitly in `main()`, and
//! database queries use raw libsql. Current version: **0.11.0**.
//!
//! ## Modules
//!
//! Foundation:
//!
//! | Module | Purpose |
//! |--------|---------|
//! | [`config`] | YAML config loader with `${VAR}` / `${VAR:default}` env substitution |
//! | [`error`] | Framework [`Error`] type (status + message + source + code) and [`Result`] alias |
//! | [`runtime`] | Graceful-shutdown [`Task`](runtime::Task) abstraction used by [`run!`] |
//! | [`server`] | HTTP server bootstrap with bind, listen, and shutdown wiring |
//! | [`service`] | Typed service [`Registry`](service::Registry) and [`AppState`](service::AppState) |
//!
//! Persistence & I/O:
//!
//! | Module | Purpose |
//! |--------|---------|
//! | [`cache`] | In-memory LRU cache with TTL |
//! | [`db`] | libsql/SQLite [`Database`](db::Database) handle, migrations, query helpers |
//! | [`storage`] | S3-compatible object storage with ACL and upload-from-URL |
//!
//! Request lifecycle:
//!
//! | Module | Purpose |
//! |--------|---------|
//! | [`client`] | HTTP client with shared connection pool |
//! | [`cookie`] | Signed/private cookie helpers and config |
//! | [`extractor`] | Request extractors with auto-sanitisation (JSON, form, query, multipart) |
//! | [`flash`] | Signed read-once cookie flash messages |
//! | [`ip`] | [`ClientIp`](ip::ClientIp) extractor with trusted-proxy resolution |
//! | [`middleware`] | Tower middleware (CORS, CSRF, rate limit, security headers, etc.) |
//! | [`sse`] | Server-Sent Events with named broadcast channels |
//!
//! Identity & multi-tenancy:
//!
//! | Module | Purpose |
//! |--------|---------|
//! | [`auth`] | Sessions (cookie + JWT), passwords, TOTP, OAuth2, API keys, RBAC roles |
//! | [`tenant`] | Multi-tenancy via subdomain, header, path, or custom resolver |
//! | [`tier`] | Subscription-tier gating |
//!
//! Background work:
//!
//! | Module | Purpose |
//! |--------|---------|
//! | [`cron`] | 5/6-field cron scheduler |
//! | [`job`] | SQLite-backed job queue with retries, scheduling, and idempotent enqueue |
//!
//! Application services:
//!
//! | Module | Purpose |
//! |--------|---------|
//! | [`email`] | Markdown-to-HTML email rendering with SMTP |
//! | [`i18n`] | ICU plural rules, locale resolution, translation store |
//! | [`qrcode`] | QR code rendering (SVG / PNG) |
//! | [`template`] | MiniJinja with i18n, HTMX detection, flash integration |
//! | [`webhook`] | Outbound webhook delivery with Standard Webhooks signing |
//!
//! Observability:
//!
//! | Module | Purpose |
//! |--------|---------|
//! | [`audit`] | Structured audit-log writer |
//! | [`health`] | `/_live` and `/_ready` endpoint handlers |
//! | [`tracing`] | Tracing/Sentry subscriber setup and request-trace middleware |
//!
//! Network primitives:
//!
//! | Module | Purpose |
//! |--------|---------|
//! | [`dns`] | TXT/CNAME verification for custom-domain validation |
//! | [`embed`] | Embed.ly-style URL preview |
//! | [`geolocation`] | MaxMind GeoIP2 lookup with middleware |
//!
//! Utilities:
//!
//! | Module | Purpose |
//! |--------|---------|
//! | [`encoding`] | Base64url, hex, and other encoding helpers |
//! | [`id`] | [`id::ulid()`](id::ulid) (26-char) and [`id::short()`](id::short) (13-char base36) |
//! | [`sanitize`] | [`Sanitize`](sanitize::Sanitize) trait used by extractors |
//! | [`validate`] | [`Validate`](validate::Validate), [`Validator`](validate::Validator), [`ValidationError`](validate::ValidationError) |
//!
//! Virtual flat-indexes (cross-module re-exports for discoverability):
//!
//! | Module | Purpose |
//! |--------|---------|
//! | [`extractors`] | Every public request extractor, gathered from across the crate |
//! | [`guards`] | Every route-level gating layer applied via `.route_layer()` |
//! | [`middlewares`] | Every public middleware constructor |
//! | [`prelude`] | Glob-import bundle for handler modules |
//!
//! Test-only (gated behind the `test-helpers` feature):
//!
//! | Module | Purpose |
//! |--------|---------|
//! | `testing` | `TestDb`, `TestApp`, `TestSession`, and in-memory/stub backends |
//!
//! ## Re-exports
//!
//! Crate-root re-exports for the items used in almost every program:
//!
//! - [`Error`] — framework error type (HTTP status + message + optional source/code)
//! - [`Result`] — `std::result::Result<T, Error>` alias
//! - [`Config`] — top-level application configuration ([`config::Config`])
//! - [`run!`] — macro that waits for SIGTERM/SIGINT then shuts down each
//!   supplied [`Task`](runtime::Task) in declaration order
//!
//! Plus the four dependency crates whose types appear in handler signatures, so
//! you don't need to pin matching versions yourself:
//!
//! - [`axum`] — router, extractors, responses
//! - [`serde`] — `Serialize` / `Deserialize` derives
//! - [`serde_json`] — JSON values and macros
//! - [`tokio`] — runtime, tasks, sync primitives
//!
//! ## Quick start
//!
//! Add to `Cargo.toml`:
//!
//! ```toml
//! [dependencies]
//! modo = { package = "modo-rs", version = "0.11.0" }
//!
//! [dev-dependencies]
//! modo = { package = "modo-rs", version = "0.11.0", features = ["test-helpers"] }
//! ```
//!
//! Minimal application:
//!
//! ```rust,no_run
//! use modo::axum::{Router, routing::get};
//! use modo::{Config, Result};
//!
//! async fn hello() -> &'static str {
//!     "Hello, modo!"
//! }
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
//! ## Features
//!
//! Every production module is always compiled — there are no per-capability
//! cargo features. The only flag is `test-helpers`, which exposes in-memory
//! backends and test harnesses; enable it in your `[dev-dependencies]`.
//!
//! | Feature | Purpose |
//! |---------|---------|
//! | `test-helpers` | Enables the `testing` module (`TestDb`, `TestApp`, `TestSession`) and all in-memory/stub backends |

pub mod config;
pub mod error;
pub mod runtime;
pub mod server;
pub mod service;

pub mod cache;
pub mod db;
pub mod storage;

pub mod client;
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

/// Top-level framework configuration deserialised from YAML — see [`config::Config`].
pub use config::Config;
/// Framework error type and `Result<T, Error>` alias — see [`error::Error`] / [`error::Result`].
pub use error::{Error, Result};

/// Re-export of the [`axum`] crate so downstream apps don't need to pin a matching version.
pub use axum;
/// Re-export of [`serde`] for `Serialize` / `Deserialize` derives in handler types.
pub use serde;
/// Re-export of [`serde_json`] for JSON values, the `json!` macro, and `Value`.
pub use serde_json;
/// Re-export of the [`tokio`] runtime — tasks, sync primitives, time, signals.
pub use tokio;
