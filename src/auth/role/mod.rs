//! # modo::auth::role
//!
//! Role-based access control for axum applications.
//!
//! Roles-only — permission checks beyond "does this role match?" belong
//! in handler logic.
//!
//! Provides:
//!
//! - [`RoleExtractor`] — trait to resolve the current user's role from a request.
//! - [`middleware()`] — Tower layer that calls your extractor and stores [`Role`] in extensions.
//! - [`Role`] — newtype extractor over `String`; pull the resolved role into handlers.
//! - [`require_role()`] — guard layer that rejects requests whose role is not in the allowed list.
//! - [`require_authenticated()`] — guard layer that rejects requests with no role at all.
//!
//! # Wiring order
//!
//! The role middleware must be applied with `.layer()` on the outer router so it runs
//! before any guard. Guards must be applied with `.route_layer()` so they execute after
//! route matching and can find the `Role` already in extensions.
//!
//! ```rust,no_run
//! use axum::{Router, routing::get};
//! use modo::auth::role::{self, RoleExtractor};
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
//!     .route_layer(role::require_role(["admin", "owner"]))
//!     .layer(role::middleware(MyExtractor));
//! ```

mod extractor;
mod guard;
mod middleware;
mod traits;

pub use extractor::Role;
pub use guard::{require_authenticated, require_role};
pub use middleware::middleware;
pub use traits::RoleExtractor;
