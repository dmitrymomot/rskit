//! Test helpers for building and exercising modo applications in-process.
//!
//! # Requires feature `test-helpers`
//!
//! **This module is gated behind the `test-helpers` feature** — the only
//! feature flag modo ships. Enable it in your `Cargo.toml` dev-dependency:
//!
//! ```toml
//! [dev-dependencies]
//! modo = { package = "modo-rs", version = "0.7", features = ["test-helpers"] }
//! ```
//!
//! Integration test files that use these helpers should also guard their
//! entire contents:
//!
//! ```rust,ignore
//! #![cfg(feature = "test-helpers")]
//! ```
//!
//! # Overview
//!
//! Lightweight utilities for integration-testing axum-based handlers without
//! spinning up a real HTTP server. Everything runs in-process using Tower's
//! [`oneshot`](tower::ServiceExt::oneshot) transport.
//!
//! # Provides
//!
//! - [`TestApp`] / [`TestAppBuilder`] — assemble a test application with routes,
//!   services, and middleware; send requests via HTTP-method helpers.
//! - [`TestDb`] — in-memory libsql database with chainable `exec` / `migrate`
//!   setup; exposes a [`Database`](crate::db::Database) handle via [`db()`](TestDb::db).
//! - [`TestPool`] — in-memory [`DatabasePool`](crate::db::DatabasePool) with chainable
//!   `exec` setup; both default and shard databases use `:memory:`.
//! - [`TestRequestBuilder`] — fluent builder for a single in-process HTTP request
//!   with JSON, form, and raw-body support.
//! - [`TestResponse`] — captured response with status, header, and body accessors.
//! - [`TestSession`] — session infrastructure for integration tests: creates the
//!   `sessions` table, signs cookies, and builds a [`SessionLayer`](crate::auth::session::SessionLayer).
//!
//! # Quick start
//!
//! ```rust,no_run
//! # #[cfg(feature = "test-helpers")]
//! # async fn example() {
//! use modo::testing::{TestApp, TestDb};
//! use axum::routing::get;
//!
//! async fn hello() -> &'static str { "hello" }
//!
//! let app = TestApp::builder()
//!     .route("/", get(hello))
//!     .build();
//!
//! let res = app.get("/").send().await;
//! assert_eq!(res.status(), 200);
//! assert_eq!(res.text(), "hello");
//! # }
//! ```

mod app;
mod db;
mod pool;
mod request;
mod response;
mod session;

pub use app::{TestApp, TestAppBuilder};
pub use db::TestDb;
pub use pool::TestPool;
pub use request::TestRequestBuilder;
pub use response::TestResponse;
pub use session::TestSession;
