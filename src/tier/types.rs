use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::error::Result;

/// Whether a feature is a boolean toggle or a usage limit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FeatureAccess {
    /// Feature is enabled or disabled.
    Toggle(bool),
    /// Feature has a usage limit ceiling.
    Limit(u64),
}

/// Resolved tier information for an owner.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    fn resolve(
        &self,
        owner_id: &str,
    ) -> Pin<Box<dyn Future<Output = Result<TierInfo>> + Send + '_>>;
}

/// Concrete wrapper around a [`TierBackend`]. `Arc` internally, cheap to clone.
#[derive(Clone)]
pub struct TierResolver(Arc<dyn TierBackend>);
