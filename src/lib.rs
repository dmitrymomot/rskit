//! # modo
//!
//! A Rust web framework for small monolithic apps.
//!
//! Single crate, zero proc macros. Handlers are plain `async fn`, routes
//! use axum's [`Router`](axum::Router) directly, services are wired
//! explicitly in `main()`, and database queries use raw libsql.
//!
//! ## Quick start
//!
//! ```toml
//! [dependencies]
//! modo = { package = "modo-rs", version = "0.7" }
//! ```
//!
//! Every module is always compiled. The only feature flag is
//! `test-helpers`, enabled in your `[dev-dependencies]`.

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
