//! Flat index of every route-level gating layer.
//!
//! Each `require_*` function returns a tower [`Layer`](tower::Layer) that
//! short-circuits the request with an [`Error`](crate::Error) response when
//! the caller fails the check. Apply them at the wiring site with
//! [`axum::Router::route_layer`] so the guard only runs for the routes it
//! protects:
//!
//! ```ignore
//! use axum::{Router, routing::get};
//! use modo::guards;
//!
//! async fn dashboard() -> &'static str { "ok" }
//!
//! let app: Router<()> = Router::new()
//!     .route("/admin", get(dashboard))
//!     .route_layer(guards::require_authenticated())
//!     .route_layer(guards::require_role(["admin"]))
//!     .route_layer(guards::require_feature("admin_panel"));
//! ```
//!
//! Available guards:
//!
//! - [`require_authenticated`] — rejects anonymous requests
//! - [`require_role`] — rejects callers missing any of the listed roles
//! - [`require_scope`] — rejects API keys without the given scope
//! - [`require_feature`] — rejects tenants whose tier lacks a feature flag
//! - [`require_limit`] — rejects tenants who would exceed a usage limit

pub use crate::auth::guard::{require_authenticated, require_role, require_scope};
pub use crate::tier::{require_feature, require_limit};
