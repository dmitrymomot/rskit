//! Flat index of every route-level gating layer.
//!
//! Wiring-site ergonomics: `.route_layer(guards::require_role(["admin"]))`.

pub use crate::auth::guard::{require_authenticated, require_role, require_scope};
pub use crate::tier::{require_feature, require_limit};
