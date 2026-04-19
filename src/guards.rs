//! Flat index of every route-level gating layer.
//!
//! Each `require_*` function returns a tower [`Layer`](tower::Layer) that
//! short-circuits the request when the caller fails the check. Apply them at
//! the wiring site with [`axum::Router::route_layer`] so the guard only runs
//! for the routes it protects:
//!
//! ```ignore
//! use axum::{Router, routing::get};
//! use modo::guards;
//!
//! async fn dashboard() -> &'static str { "ok" }
//! async fn login() -> &'static str { "login" }
//!
//! let app: Router<()> = Router::new()
//!     .route("/app", get(dashboard))
//!     .route_layer(guards::require_authenticated("/auth"))
//!     .route("/auth", get(login))
//!     .route_layer(guards::require_unauthenticated("/app"));
//! ```
//!
//! Available guards:
//!
//! - [`require_authenticated`] — redirects anonymous callers to a login path
//! - [`require_unauthenticated`] — redirects signed-in callers away from guest-only routes
//! - [`require_role`] — rejects callers missing any of the listed roles
//! - [`require_scope`] — rejects API keys without the given scope
//! - [`require_feature`] — rejects tenants whose tier lacks a feature flag
//! - [`require_limit`] — rejects tenants who would exceed a usage limit

pub use crate::auth::guard::{
    require_authenticated, require_role, require_scope, require_unauthenticated,
};
pub use crate::tier::{require_feature, require_limit};
