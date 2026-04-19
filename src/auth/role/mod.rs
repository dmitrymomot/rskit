//! # modo::auth::role
//!
//! Role-based gating for axum applications.
//!
//! This module is **roles-only**: it resolves a single role string per request
//! and makes it available to handlers and to the route-level `require_role`
//! guard. Any richer authorization (per-resource permissions, scopes,
//! ownership checks, ABAC rules) belongs in your handler code — modo does not
//! model permissions.
//!
//! Provides:
//!
//! - [`RoleExtractor`] — trait you implement to resolve the caller's role from
//!   a request (session lookup, JWT claim, API key metadata, etc.).
//! - [`middleware()`] — Tower layer that runs the extractor and stores the
//!   resulting [`Role`] in request extensions.
//! - [`Role`] — newtype over `String` implementing axum's
//!   `FromRequestParts` and `OptionalFromRequestParts`; pull it into handlers
//!   as `Role` or `Option<Role>`.
//!
//! Route-level gating layers (`require_role`, `require_authenticated`,
//! `require_unauthenticated`, `require_scope`) live in
//! [`crate::auth::guard`]. This module only resolves the role; `auth::guard`
//! compares it against an allow-list.
//!
//! # Wiring order
//!
//! The role middleware must be applied with `.layer()` on the outer router so it runs
//! before any guard. Guards must be applied with `.route_layer()` so they execute after
//! route matching and can find the `Role` already in extensions.
//!
//! ```rust,no_run
//! use axum::{Router, routing::get};
//! use modo::auth::{guard, role::{self, RoleExtractor}};
//! use modo::Result;
//!
//! struct MyExtractor;
//!
//! impl RoleExtractor for MyExtractor {
//!     async fn extract(&self, _parts: &mut http::request::Parts) -> Result<String> {
//!         Ok("admin".to_string())
//!     }
//! }
//!
//! let app: Router = Router::new()
//!     .route("/admin", get(|| async { "ok" }))
//!     .route_layer(guard::require_role(["admin", "owner"]))
//!     .layer(role::middleware(MyExtractor));
//! ```

mod extractor;
mod middleware;
mod traits;

pub use extractor::Role;
pub use middleware::middleware;
pub use traits::RoleExtractor;
