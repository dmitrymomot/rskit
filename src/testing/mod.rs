//! # modo::testing
//!
//! In-process test harness for modo applications.
//!
//! **Requires feature `test-helpers`.** This is the only runtime feature modo
//! ships — it gates every item in this module along with the in-memory /
//! stub backends that tests rely on. Enable it as a dev-dependency:
//!
//! ```toml
//! [dev-dependencies]
//! modo-rs = { version = "0.10.4", features = ["test-helpers"] }
//! ```
//!
//! Integration test files that import from `modo::testing` should guard
//! their contents:
//!
//! ```rust,ignore
//! #![cfg(feature = "test-helpers")]
//! ```
//!
//! The helpers dispatch requests in-process through Tower's
//! [`oneshot`](tower::ServiceExt::oneshot), so no sockets are opened and no
//! runtime server is spawned.
//!
//! # Provides
//!
//! - [`TestApp`] / [`TestAppBuilder`] — assemble a test application with
//!   routes, services, and middleware; dispatch requests via HTTP-method
//!   helpers (`get`, `post`, `put`, `patch`, `delete`, `options`, `request`).
//! - [`TestDb`] — in-memory libsql database with chainable [`exec`](TestDb::exec)
//!   / [`migrate`](TestDb::migrate) setup; exposes a
//!   [`Database`](crate::db::Database) handle via [`db()`](TestDb::db).
//! - [`TestPool`] — in-memory [`DatabasePool`](crate::db::DatabasePool) with
//!   chainable [`exec`](TestPool::exec) setup; both the default database and
//!   all shards use `:memory:`.
//! - [`TestRequestBuilder`] — fluent builder for a single in-process HTTP
//!   request with JSON, form, raw-body, and header helpers.
//! - [`TestResponse`] — captured response with status, header, text, JSON,
//!   and raw-bytes accessors.
//! - [`TestSession`] — session infrastructure for integration tests: creates
//!   the `authenticated_sessions` table (see [`TestSession::SCHEMA_SQL`] and
//!   [`TestSession::INDEXES_SQL`]), signs cookies, and builds a
//!   [`CookieSessionLayer`](crate::auth::session::CookieSessionLayer).
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
