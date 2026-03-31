use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// Whether a feature is a boolean toggle or a usage limit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FeatureAccess {
    /// Feature is enabled or disabled.
    Toggle(bool),
    /// Feature has a usage limit ceiling.
    Limit(u64),
}

/// Resolved tier information for an owner.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TierInfo {
    /// Plan name (e.g., "free", "pro", "enterprise").
    pub name: String,
    /// Feature map: feature name → access level.
    pub features: HashMap<String, FeatureAccess>,
}

/// Backend trait for tier resolution. Object-safe.
///
/// The app implements this with its own storage/logic — the framework
/// provides the trait, wrapper, middleware, and guards.
pub trait TierBackend: Send + Sync {
    /// Resolve tier information for the given owner.
    ///
    /// # Errors
    ///
    /// Implementation-defined. Errors are surfaced by
    /// [`TierLayer`](super::TierLayer) as HTTP error responses.
    fn resolve(
        &self,
        owner_id: &str,
    ) -> Pin<Box<dyn Future<Output = Result<TierInfo>> + Send + '_>>;
}

/// Concrete wrapper around a [`TierBackend`]. `Arc` internally, cheap to clone.
#[derive(Clone)]
pub struct TierResolver(Arc<dyn TierBackend>);

impl TierInfo {
    /// Feature is available (Toggle=true or Limit>0).
    pub fn has_feature(&self, name: &str) -> bool {
        match self.features.get(name) {
            Some(FeatureAccess::Toggle(v)) => *v,
            Some(FeatureAccess::Limit(v)) => *v > 0,
            None => false,
        }
    }

    /// Feature is explicitly enabled (Toggle only, false for Limit or missing).
    pub fn is_enabled(&self, name: &str) -> bool {
        matches!(self.features.get(name), Some(FeatureAccess::Toggle(true)))
    }

    /// Get the limit ceiling (Limit only, None for Toggle or missing).
    pub fn limit(&self, name: &str) -> Option<u64> {
        match self.features.get(name) {
            Some(FeatureAccess::Limit(v)) => Some(*v),
            _ => None,
        }
    }

    /// Get the limit ceiling, returning typed errors for missing or non-limit features.
    ///
    /// Returns `Ok(ceiling)` for `Limit` features.
    ///
    /// # Errors
    ///
    /// - [`Error::forbidden`](crate::Error::forbidden) if the feature is missing.
    /// - [`Error::internal`](crate::Error::internal) if the feature is a `Toggle` (not a limit).
    pub fn limit_ceiling(&self, name: &str) -> Result<u64> {
        match self.features.get(name) {
            Some(FeatureAccess::Limit(v)) => Ok(*v),
            Some(FeatureAccess::Toggle(_)) => {
                Err(Error::internal(format!("Feature '{name}' is not a limit")))
            }
            None => Err(Error::forbidden(format!(
                "Feature '{name}' is not available on your current plan"
            ))),
        }
    }

    /// Check current usage against limit ceiling.
    ///
    /// Returns `Ok(())` if usage is under the limit.
    ///
    /// # Errors
    ///
    /// - [`Error::forbidden`](crate::Error::forbidden) if the feature is missing or usage >= limit.
    /// - [`Error::internal`](crate::Error::internal) if the feature is a `Toggle` (not a limit).
    pub fn check_limit(&self, name: &str, current: u64) -> Result<()> {
        let ceiling = self.limit_ceiling(name)?;
        if current >= ceiling {
            Err(Error::forbidden(format!(
                "Limit exceeded for '{name}': {current}/{ceiling}"
            )))
        } else {
            Ok(())
        }
    }
}

impl TierResolver {
    /// Create from a custom backend.
    pub fn from_backend(backend: Arc<dyn TierBackend>) -> Self {
        Self(backend)
    }

    /// Resolve tier information for an owner.
    ///
    /// # Errors
    ///
    /// Returns any error produced by the underlying [`TierBackend`].
    pub async fn resolve(&self, owner_id: &str) -> Result<TierInfo> {
        self.0.resolve(owner_id).await
    }
}

/// Test helpers for the tier module.
///
/// Available when running tests or when the `test-helpers` feature is enabled.
#[cfg_attr(not(any(test, feature = "test-helpers")), allow(dead_code))]
pub mod test_support {
    use super::*;

    /// In-memory backend that returns a fixed `TierInfo` for any owner ID.
    pub struct StaticTierBackend {
        tier: TierInfo,
    }

    impl StaticTierBackend {
        /// Create a backend that always returns the given tier.
        pub fn new(tier: TierInfo) -> Self {
            Self { tier }
        }
    }

    impl TierBackend for StaticTierBackend {
        fn resolve(
            &self,
            _owner_id: &str,
        ) -> Pin<Box<dyn Future<Output = Result<TierInfo>> + Send + '_>> {
            Box::pin(async { Ok(self.tier.clone()) })
        }
    }

    /// In-memory backend that always returns an error.
    pub struct FailingTierBackend;

    impl TierBackend for FailingTierBackend {
        fn resolve(
            &self,
            _owner_id: &str,
        ) -> Pin<Box<dyn Future<Output = Result<TierInfo>> + Send + '_>> {
            Box::pin(async { Err(Error::internal("test: backend failure")) })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn free_tier() -> TierInfo {
        TierInfo {
            name: "free".into(),
            features: HashMap::from([
                ("basic_export".into(), FeatureAccess::Toggle(true)),
                ("sso".into(), FeatureAccess::Toggle(false)),
                ("api_calls".into(), FeatureAccess::Limit(1_000)),
                ("storage_mb".into(), FeatureAccess::Limit(0)),
            ]),
        }
    }

    fn pro_tier() -> TierInfo {
        TierInfo {
            name: "pro".into(),
            features: HashMap::from([
                ("basic_export".into(), FeatureAccess::Toggle(true)),
                ("sso".into(), FeatureAccess::Toggle(true)),
                ("api_calls".into(), FeatureAccess::Limit(100_000)),
            ]),
        }
    }

    // --- has_feature ---

    #[test]
    fn has_feature_toggle_true() {
        assert!(free_tier().has_feature("basic_export"));
    }

    #[test]
    fn has_feature_toggle_false() {
        assert!(!free_tier().has_feature("sso"));
    }

    #[test]
    fn has_feature_limit_positive() {
        assert!(free_tier().has_feature("api_calls"));
    }

    #[test]
    fn has_feature_limit_zero() {
        assert!(!free_tier().has_feature("storage_mb"));
    }

    #[test]
    fn has_feature_missing() {
        assert!(!free_tier().has_feature("nonexistent"));
    }

    // --- is_enabled ---

    #[test]
    fn is_enabled_toggle_true() {
        assert!(pro_tier().is_enabled("sso"));
    }

    #[test]
    fn is_enabled_toggle_false() {
        assert!(!free_tier().is_enabled("sso"));
    }

    #[test]
    fn is_enabled_limit_returns_false() {
        assert!(!free_tier().is_enabled("api_calls"));
    }

    #[test]
    fn is_enabled_missing_returns_false() {
        assert!(!free_tier().is_enabled("nonexistent"));
    }

    // --- limit ---

    #[test]
    fn limit_returns_ceiling() {
        assert_eq!(free_tier().limit("api_calls"), Some(1_000));
    }

    #[test]
    fn limit_toggle_returns_none() {
        assert_eq!(free_tier().limit("basic_export"), None);
    }

    #[test]
    fn limit_missing_returns_none() {
        assert_eq!(free_tier().limit("nonexistent"), None);
    }

    // --- check_limit ---

    #[test]
    fn check_limit_under_ok() {
        assert!(free_tier().check_limit("api_calls", 500).is_ok());
    }

    #[test]
    fn check_limit_at_ceiling_forbidden() {
        let err = free_tier().check_limit("api_calls", 1_000).unwrap_err();
        assert_eq!(err.status(), http::StatusCode::FORBIDDEN);
    }

    #[test]
    fn check_limit_over_ceiling_forbidden() {
        let err = free_tier().check_limit("api_calls", 2_000).unwrap_err();
        assert_eq!(err.status(), http::StatusCode::FORBIDDEN);
    }

    #[test]
    fn check_limit_toggle_internal_error() {
        let err = free_tier().check_limit("basic_export", 0).unwrap_err();
        assert_eq!(err.status(), http::StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn check_limit_missing_forbidden() {
        let err = free_tier().check_limit("nonexistent", 0).unwrap_err();
        assert_eq!(err.status(), http::StatusCode::FORBIDDEN);
    }

    // --- FeatureAccess serde ---

    #[test]
    fn feature_access_toggle_roundtrip() {
        let v = FeatureAccess::Toggle(true);
        let json = serde_json::to_string(&v).unwrap();
        let back: FeatureAccess = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, FeatureAccess::Toggle(true)));
    }

    #[test]
    fn feature_access_limit_roundtrip() {
        let v = FeatureAccess::Limit(5_000);
        let json = serde_json::to_string(&v).unwrap();
        let back: FeatureAccess = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, FeatureAccess::Limit(5_000)));
    }

    #[test]
    fn tier_info_serde_roundtrip() {
        let tier = free_tier();
        let json = serde_json::to_string(&tier).unwrap();
        let back: TierInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, "free");
        assert!(back.has_feature("basic_export"));
        assert!(!back.has_feature("sso"));
    }

    // --- TierResolver ---

    struct StaticBackend(TierInfo);

    impl TierBackend for StaticBackend {
        fn resolve(
            &self,
            _owner_id: &str,
        ) -> Pin<Box<dyn Future<Output = Result<TierInfo>> + Send + '_>> {
            Box::pin(async { Ok(self.0.clone()) })
        }
    }

    #[tokio::test]
    async fn resolver_delegates_to_backend() {
        let resolver = TierResolver::from_backend(Arc::new(StaticBackend(pro_tier())));
        let info = resolver.resolve("tenant_123").await.unwrap();
        assert_eq!(info.name, "pro");
    }
}
