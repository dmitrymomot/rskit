//! Role-based access control (RBAC) for axum applications.
//!
//! Three building blocks work together:
//!
//! - [`RoleExtractor`] — implement this to resolve the current user's role from a request.
//! - [`middleware()`] — Tower layer that calls your extractor and stores [`Role`] in extensions.
//! - [`require_role()`] / [`require_authenticated()`] — guard layers applied with
//!   `.route_layer()` that reject requests before they reach handlers.
//!
//! # Wiring order
//!
//! The RBAC middleware must be applied with `.layer()` on the outer router so it runs
//! before any guard. Guards must be applied with `.route_layer()` so they execute after
//! route matching and can find the `Role` already in extensions.
//!
//! ```rust,no_run
//! use axum::{Router, routing::get};
//! use modo::rbac::{self, RoleExtractor};
//! use modo::Result;
//!
//! struct MyExtractor;
//!
//! impl RoleExtractor for MyExtractor {
//!     async fn extract(&self, parts: &mut http::request::Parts) -> Result<String> {
//!         Ok("admin".to_string())
//!     }
//! }
//!
//! let app: Router = Router::new()
//!     .route("/admin", get(|| async { "ok" }))
//!     .route_layer(rbac::require_role(["admin", "owner"]))
//!     .layer(rbac::middleware(MyExtractor));
//! ```

mod extractor;
mod guard;
mod middleware;
mod traits;

pub use extractor::Role;
pub use guard::{require_authenticated, require_role};
pub use middleware::middleware;
pub use traits::RoleExtractor;
