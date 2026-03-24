//! Test helpers for building and exercising modo applications in-process.
//!
//! This module provides lightweight utilities for integration-testing axum-based
//! handlers without spinning up a real HTTP server. Everything runs in-process
//! using Tower's [`oneshot`](tower::ServiceExt::oneshot) transport.
//!
//! # Requires feature `test-helpers`
//!
//! Enable the feature in your `Cargo.toml` dev-dependency:
//!
//! ```toml
//! [dev-dependencies]
//! modo = { path = "..", features = ["test-helpers"] }
//! ```
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
mod request;
mod response;
mod session;

pub use app::{TestApp, TestAppBuilder};
pub use db::TestDb;
pub use request::TestRequestBuilder;
pub use response::TestResponse;
pub use session::TestSession;
