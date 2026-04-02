//! # modo::tier
//!
//! Tier-based feature gating for SaaS applications.
//!
//! Requires feature `"tier"`.
//!
//! ```toml
//! [dependencies]
//! modo = { version = "0.5", features = ["tier"] }
//! ```
//!
//! ## Provides
//!
//! - [`TierBackend`] — trait for pluggable tier resolution (app implements)
//! - [`TierResolver`] — concrete wrapper (`Arc<dyn TierBackend>`, cheap to clone)
//! - [`TierInfo`] — resolved tier with feature checks
//! - [`FeatureAccess`] — toggle or limit feature model
//! - [`TierLayer`] — Tower middleware that resolves and injects [`TierInfo`]
//! - [`require_feature()`] — route guard for boolean feature gates
//! - [`require_limit()`] — route guard for usage-limit gates
//! - [`mod@test`] — test helpers (`StaticTierBackend`, `FailingTierBackend`)
//!
//! ## Quick start
//!
//! ```rust,ignore
//! use modo::tier::{TierBackend, TierResolver, TierInfo, TierLayer, require_feature};
//! use axum::{Router, routing::get};
//!
//! # fn example(resolver: TierResolver) {
//! let app: Router = Router::new()
//!     .route("/settings/domain", get(|| async { "ok" }))
//!     .route_layer(require_feature("custom_domain"))
//!     .layer(TierLayer::new(resolver, |parts| {
//!         parts.extensions.get::<modo::TenantId>().map(|id| id.as_str().to_owned())
//!     }));
//! # }
//! ```

mod extractor;
mod guard;
mod middleware;
mod types;

pub use extractor::TierInfo;
pub use guard::{require_feature, require_limit};
pub use middleware::TierLayer;
pub use types::{FeatureAccess, TierBackend, TierResolver};

/// Test helpers for the tier module.
///
/// Available when running tests or when the `test-helpers` feature is enabled.
#[cfg_attr(not(any(test, feature = "test-helpers")), allow(dead_code))]
pub mod test {
    pub use super::types::test_support::{FailingTierBackend, StaticTierBackend};
}
