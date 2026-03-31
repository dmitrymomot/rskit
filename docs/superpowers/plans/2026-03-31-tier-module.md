# Tier Module Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a `tier` module to modo that provides plan-based feature gating for SaaS apps via a pluggable backend trait, Tower middleware, route-level guards, and template integration.

**Architecture:** `TierBackend` trait (object-safe, `Arc<dyn>`) resolved by `TierResolver` wrapper. `TierLayer` middleware takes a sync closure for owner ID extraction, resolves `TierInfo`, inserts into extensions. Guards (`require_feature`, `require_limit`) enforce at route level. Template functions injected via `#[cfg(feature = "tier")]` in `TemplateContextMiddleware`.

**Tech Stack:** Rust, axum 0.8, tower, serde, std::collections::HashMap

**Spec:** `docs/superpowers/specs/2026-03-31-tier-module-design.md`

---

## File Map

| File | Responsibility |
|------|---------------|
| `src/tier/mod.rs` | Module declaration, re-exports |
| `src/tier/types.rs` | `FeatureAccess`, `TierInfo`, `TierBackend`, `TierResolver`, in-memory test backend |
| `src/tier/extractor.rs` | `TierInfo` `FromRequestParts` + `OptionalFromRequestParts` |
| `src/tier/middleware.rs` | `TierLayer`, `TierMiddleware` |
| `src/tier/guard.rs` | `require_feature()`, `require_limit()` |
| `src/lib.rs` | Add `#[cfg(feature = "tier")] pub mod tier;` + re-exports |
| `Cargo.toml` | Add `tier = []` feature, add to `full` |
| `src/template/middleware.rs` | Add `#[cfg(feature = "tier")]` block for template context injection |

---

## Task 1: Feature Flag & Module Skeleton

**Files:**
- Modify: `Cargo.toml` (features section)
- Modify: `src/lib.rs`
- Create: `src/tier/mod.rs`

- [ ] **Step 1: Add feature flag to `Cargo.toml`**

In the `[features]` section, add `tier = []` and include it in `full`:

```toml
tier = []
```

Update the `full` feature to include `"tier"`:

```toml
full = ["db", "session", "job", "http-client", "templates", "sse", "auth", "sentry", "email", "storage", "webhooks", "dns", "geolocation", "qrcode", "apikey", "text-embedding", "tier"]
```

- [ ] **Step 2: Add module to `src/lib.rs`**

Add the conditional module declaration after the `apikey` block:

```rust
#[cfg(feature = "tier")]
pub mod tier;
```

- [ ] **Step 3: Create `src/tier/mod.rs`**

```rust
//! Tier-based feature gating for SaaS applications.
//!
//! Requires feature `"tier"`.
//!
//! ```toml
//! [dependencies]
//! modo = { version = "*", features = ["tier"] }
//! ```
//!
//! Provides:
//!
//! - [`TierBackend`] — trait for pluggable tier resolution (app implements)
//! - [`TierResolver`] — concrete wrapper (`Arc<dyn TierBackend>`, cheap to clone)
//! - [`TierInfo`] — resolved tier with feature checks
//! - [`FeatureAccess`] — toggle or limit feature model
//! - [`TierLayer`] — Tower middleware that resolves and injects `TierInfo`
//! - [`require_feature()`] — route guard for boolean feature gates
//! - [`require_limit()`] — route guard for usage limit gates
//!
//! ## Quick start
//!
//! ```rust,no_run
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
```

- [ ] **Step 4: Create placeholder files so the module compiles**

Create empty placeholder files that will be populated in subsequent tasks. Each file needs minimal content to compile:

`src/tier/types.rs`:
```rust
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

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
```

`src/tier/extractor.rs`:
```rust
pub use super::types::TierInfo;
```

`src/tier/guard.rs`:
```rust
pub fn require_feature(_name: &str) -> RequireFeatureLayer {
    todo!()
}

pub fn require_limit<F, Fut>(_name: &str, _usage: F) -> RequireLimitLayer
where
    F: Send,
    Fut: Send,
{
    todo!()
}

pub struct RequireFeatureLayer;
pub struct RequireLimitLayer;
```

`src/tier/middleware.rs`:
```rust
pub struct TierLayer;
```

These are placeholders — each subsequent task replaces them with full implementations.

- [ ] **Step 5: Verify it compiles**

Run: `cargo check --features tier`
Expected: compiles with no errors (placeholders are unused but should parse)

- [ ] **Step 6: Commit**

```bash
git add src/tier/mod.rs src/tier/types.rs src/tier/extractor.rs src/tier/guard.rs src/tier/middleware.rs Cargo.toml src/lib.rs
git commit -m "feat(tier): add module skeleton and feature flag"
```

---

## Task 2: Types — `TierInfo` Methods & `TierResolver`

**Files:**
- Modify: `src/tier/types.rs`

- [ ] **Step 1: Write tests for `TierInfo` methods**

Add to the bottom of `src/tier/types.rs`:

```rust
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

    /// Check current usage against limit ceiling.
    ///
    /// Returns `Ok(())` if usage is under the limit.
    /// Returns `Err(forbidden)` if the feature is missing, disabled, or usage >= limit.
    /// Returns `Err(internal)` if the feature is a Toggle (not a limit).
    pub fn check_limit(&self, name: &str, current: u64) -> Result<()> {
        match self.features.get(name) {
            None => Err(Error::forbidden(format!(
                "Feature '{name}' is not available on your current plan"
            ))),
            Some(FeatureAccess::Toggle(_)) => Err(Error::internal(format!(
                "Feature '{name}' is not a limit"
            ))),
            Some(FeatureAccess::Limit(ceiling)) => {
                if current >= *ceiling {
                    Err(Error::forbidden(format!(
                        "Limit exceeded for '{name}': {current}/{ceiling}"
                    )))
                } else {
                    Ok(())
                }
            }
        }
    }
}

impl TierResolver {
    /// Create from a custom backend.
    pub fn from_backend(backend: Arc<dyn TierBackend>) -> Self {
        Self(backend)
    }

    /// Resolve tier information for an owner.
    pub async fn resolve(&self, owner_id: &str) -> Result<TierInfo> {
        self.0.resolve(owner_id).await
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
```

- [ ] **Step 2: Run tests**

Run: `cargo test --features tier tier::types`
Expected: all tests pass

- [ ] **Step 3: Commit**

```bash
git add src/tier/types.rs
git commit -m "feat(tier): implement TierInfo, FeatureAccess, TierBackend, TierResolver"
```

---

## Task 3: Extractor — `TierInfo` FromRequestParts

**Files:**
- Modify: `src/tier/extractor.rs`

- [ ] **Step 1: Implement the extractor**

Replace the placeholder in `src/tier/extractor.rs` with:

```rust
use axum::extract::{FromRequestParts, OptionalFromRequestParts};
use http::request::Parts;

use crate::error::Error;

pub use super::types::TierInfo;

impl<S: Send + Sync> FromRequestParts<S> for TierInfo {
    type Rejection = Error;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<TierInfo>()
            .cloned()
            .ok_or_else(|| Error::internal("Tier middleware not applied"))
    }
}

impl<S: Send + Sync> OptionalFromRequestParts<S> for TierInfo {
    type Rejection = Error;

    async fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> Result<Option<Self>, Self::Rejection> {
        Ok(parts.extensions.get::<TierInfo>().cloned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    use super::super::types::FeatureAccess;

    fn test_tier() -> TierInfo {
        TierInfo {
            name: "pro".into(),
            features: HashMap::from([
                ("sso".into(), FeatureAccess::Toggle(true)),
            ]),
        }
    }

    #[tokio::test]
    async fn extract_from_extensions() {
        let (mut parts, _) = http::Request::builder().body(()).unwrap().into_parts();
        parts.extensions.insert(test_tier());

        let result = <TierInfo as FromRequestParts<()>>::from_request_parts(&mut parts, &()).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().name, "pro");
    }

    #[tokio::test]
    async fn extract_missing_returns_500() {
        let (mut parts, _) = http::Request::builder().body(()).unwrap().into_parts();

        let result = <TierInfo as FromRequestParts<()>>::from_request_parts(&mut parts, &()).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.status(), http::StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn optional_none_when_missing() {
        let (mut parts, _) = http::Request::builder().body(()).unwrap().into_parts();

        let result =
            <TierInfo as OptionalFromRequestParts<()>>::from_request_parts(&mut parts, &()).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[tokio::test]
    async fn optional_some_when_present() {
        let (mut parts, _) = http::Request::builder().body(()).unwrap().into_parts();
        parts.extensions.insert(test_tier());

        let result =
            <TierInfo as OptionalFromRequestParts<()>>::from_request_parts(&mut parts, &()).await;
        assert!(result.is_ok());
        let tier = result.unwrap().unwrap();
        assert_eq!(tier.name, "pro");
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test --features tier tier::extractor`
Expected: all 4 tests pass

- [ ] **Step 3: Commit**

```bash
git add src/tier/extractor.rs
git commit -m "feat(tier): implement TierInfo FromRequestParts extractor"
```

---

## Task 4: Middleware — `TierLayer` & `TierMiddleware`

**Files:**
- Modify: `src/tier/middleware.rs`

- [ ] **Step 1: Implement the middleware**

Replace the placeholder in `src/tier/middleware.rs` with:

```rust
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use axum::body::Body;
use axum::response::IntoResponse;
use http::request::Parts;
use http::Request;
use tower::{Layer, Service};

use super::types::{TierInfo, TierResolver};

type OwnerExtractor = Arc<dyn Fn(&Parts) -> Option<String> + Send + Sync>;

/// Tower middleware layer that resolves [`TierInfo`] and inserts it into
/// request extensions.
///
/// Apply with `.layer()` on the router. Guards ([`super::require_feature`],
/// [`super::require_limit`]) are applied separately with `.route_layer()`.
///
/// # Owner ID extraction
///
/// The extractor closure reads from `&Parts` (populated by upstream middleware)
/// and returns `Some(owner_id)` or `None`.
///
/// # Default tier
///
/// When the extractor returns `None` and a default is set via
/// [`with_default`](Self::with_default), the default `TierInfo` is inserted.
/// Otherwise, no `TierInfo` is inserted and the inner service is called
/// directly — downstream guards handle the absence.
pub struct TierLayer {
    resolver: TierResolver,
    extractor: OwnerExtractor,
    default: Option<TierInfo>,
}

impl TierLayer {
    /// Create a new tier layer.
    ///
    /// `extractor` is a sync closure that returns the owner ID from request
    /// parts, or `None` if no owner context is available.
    pub fn new<F>(resolver: TierResolver, extractor: F) -> Self
    where
        F: Fn(&Parts) -> Option<String> + Send + Sync + 'static,
    {
        Self {
            resolver,
            extractor: Arc::new(extractor),
            default: None,
        }
    }

    /// When the extractor returns `None`, inject this `TierInfo` instead of
    /// skipping.
    pub fn with_default(mut self, default: TierInfo) -> Self {
        self.default = Some(default);
        self
    }
}

impl Clone for TierLayer {
    fn clone(&self) -> Self {
        Self {
            resolver: self.resolver.clone(),
            extractor: self.extractor.clone(),
            default: self.default.clone(),
        }
    }
}

impl<S> Layer<S> for TierLayer {
    type Service = TierMiddleware<S>;

    fn layer(&self, inner: S) -> Self::Service {
        TierMiddleware {
            inner,
            resolver: self.resolver.clone(),
            extractor: self.extractor.clone(),
            default: self.default.clone(),
        }
    }
}

/// Tower service produced by [`TierLayer`].
pub struct TierMiddleware<S> {
    inner: S,
    resolver: TierResolver,
    extractor: OwnerExtractor,
    default: Option<TierInfo>,
}

impl<S: Clone> Clone for TierMiddleware<S> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            resolver: self.resolver.clone(),
            extractor: self.extractor.clone(),
            default: self.default.clone(),
        }
    }
}

impl<S> Service<Request<Body>> for TierMiddleware<S>
where
    S: Service<Request<Body>, Response = http::Response<Body>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Into<Box<dyn std::error::Error + Send + Sync>> + Send + 'static,
{
    type Response = http::Response<Body>;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request<Body>) -> Self::Future {
        let resolver = self.resolver.clone();
        let extractor = self.extractor.clone();
        let default = self.default.clone();
        let mut inner = self.inner.clone();
        std::mem::swap(&mut self.inner, &mut inner);

        Box::pin(async move {
            let (mut parts, body) = request.into_parts();

            let tier_info = match (extractor)(&parts) {
                Some(owner_id) => match resolver.resolve(&owner_id).await {
                    Ok(info) => Some(info),
                    Err(e) => return Ok(e.into_response()),
                },
                None => default,
            };

            if let Some(info) = tier_info {
                parts.extensions.insert(info);
            }

            let request = Request::from_parts(parts, body);
            inner.call(request).await
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::convert::Infallible;

    use http::{Response, StatusCode};
    use tower::ServiceExt;

    use crate::error::Error;
    use super::super::types::{FeatureAccess, TierBackend};

    fn pro_tier() -> TierInfo {
        TierInfo {
            name: "pro".into(),
            features: HashMap::from([
                ("sso".into(), FeatureAccess::Toggle(true)),
            ]),
        }
    }

    fn anon_tier() -> TierInfo {
        TierInfo {
            name: "anonymous".into(),
            features: HashMap::from([
                ("public_api".into(), FeatureAccess::Toggle(true)),
            ]),
        }
    }

    struct StaticBackend(TierInfo);

    impl TierBackend for StaticBackend {
        fn resolve(
            &self,
            _owner_id: &str,
        ) -> Pin<Box<dyn Future<Output = crate::Result<TierInfo>> + Send + '_>> {
            Box::pin(async { Ok(self.0.clone()) })
        }
    }

    struct FailingBackend;

    impl TierBackend for FailingBackend {
        fn resolve(
            &self,
            _owner_id: &str,
        ) -> Pin<Box<dyn Future<Output = crate::Result<TierInfo>> + Send + '_>> {
            Box::pin(async { Err(Error::internal("db is down")) })
        }
    }

    fn resolver(tier: TierInfo) -> TierResolver {
        TierResolver::from_backend(Arc::new(StaticBackend(tier)))
    }

    fn failing_resolver() -> TierResolver {
        TierResolver::from_backend(Arc::new(FailingBackend))
    }

    async fn ok_handler(req: Request<Body>) -> Result<Response<Body>, Infallible> {
        let has_tier = req.extensions().get::<TierInfo>().is_some();
        let body = if has_tier { "tier-present" } else { "no-tier" };
        Ok(Response::new(Body::from(body)))
    }

    #[tokio::test]
    async fn extractor_some_resolves_tier() {
        let layer = TierLayer::new(resolver(pro_tier()), |_| Some("tenant_1".into()));
        let svc = layer.layer(tower::service_fn(ok_handler));

        let req = Request::builder().body(Body::empty()).unwrap();
        let resp = svc.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        assert_eq!(body, "tier-present");
    }

    #[tokio::test]
    async fn extractor_none_no_default_skips() {
        let layer = TierLayer::new(resolver(pro_tier()), |_| None);
        let svc = layer.layer(tower::service_fn(ok_handler));

        let req = Request::builder().body(Body::empty()).unwrap();
        let resp = svc.oneshot(req).await.unwrap();

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        assert_eq!(body, "no-tier");
    }

    #[tokio::test]
    async fn extractor_none_with_default_injects_default() {
        let layer = TierLayer::new(resolver(pro_tier()), |_| None)
            .with_default(anon_tier());
        let svc = layer.layer(tower::service_fn(ok_handler));

        let req = Request::builder().body(Body::empty()).unwrap();
        let resp = svc.oneshot(req).await.unwrap();

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        assert_eq!(body, "tier-present");
    }

    #[tokio::test]
    async fn backend_error_returns_error_response() {
        let layer = TierLayer::new(failing_resolver(), |_| Some("tenant_1".into()));
        let svc = layer.layer(tower::service_fn(ok_handler));

        let req = Request::builder().body(Body::empty()).unwrap();
        let resp = svc.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn backend_error_does_not_call_inner() {
        use std::sync::atomic::{AtomicBool, Ordering};

        let called = Arc::new(AtomicBool::new(false));
        let called_clone = called.clone();

        let layer = TierLayer::new(failing_resolver(), |_| Some("tenant_1".into()));
        let svc = layer.layer(tower::service_fn(move |_req: Request<Body>| {
            let called = called_clone.clone();
            async move {
                called.store(true, Ordering::SeqCst);
                Ok::<_, Infallible>(Response::new(Body::from("should not reach")))
            }
        }));

        let req = Request::builder().body(Body::empty()).unwrap();
        let _resp = svc.oneshot(req).await.unwrap();
        assert!(!called.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn tier_info_accessible_in_inner_service() {
        let layer = TierLayer::new(resolver(pro_tier()), |_| Some("t".into()));

        let inner = tower::service_fn(|req: Request<Body>| async move {
            let tier = req.extensions().get::<TierInfo>().unwrap();
            assert_eq!(tier.name, "pro");
            assert!(tier.has_feature("sso"));
            Ok::<_, Infallible>(Response::new(Body::empty()))
        });

        let svc = layer.layer(inner);
        let req = Request::builder().body(Body::empty()).unwrap();
        let resp = svc.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn extractor_reads_from_extensions() {
        #[derive(Clone)]
        struct OwnerId(String);

        let layer = TierLayer::new(resolver(pro_tier()), |parts| {
            parts.extensions.get::<OwnerId>().map(|id| id.0.clone())
        });
        let svc = layer.layer(tower::service_fn(ok_handler));

        let mut req = Request::builder().body(Body::empty()).unwrap();
        req.extensions_mut().insert(OwnerId("owner_42".into()));
        let resp = svc.oneshot(req).await.unwrap();

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        assert_eq!(body, "tier-present");
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test --features tier tier::middleware`
Expected: all 7 tests pass

- [ ] **Step 3: Commit**

```bash
git add src/tier/middleware.rs
git commit -m "feat(tier): implement TierLayer and TierMiddleware"
```

---

## Task 5: Guards — `require_feature()` & `require_limit()`

**Files:**
- Modify: `src/tier/guard.rs`

- [ ] **Step 1: Implement the guards**

Replace the placeholder in `src/tier/guard.rs` with:

```rust
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use axum::body::Body;
use axum::response::IntoResponse;
use http::Request;
use http::request::Parts;
use tower::{Layer, Service};

use crate::error::Error;

use super::types::{FeatureAccess, TierInfo};

// ---------------------------------------------------------------------------
// require_feature
// ---------------------------------------------------------------------------

/// Creates a guard layer that rejects requests unless the resolved tier
/// includes the named feature and it is available.
///
/// Apply with `.route_layer()` so it runs after route matching.
/// [`TierLayer`](super::TierLayer) must be applied with `.layer()` so that
/// `TierInfo` is in extensions when this guard runs.
///
/// - `TierInfo` missing → `Error::internal` (developer misconfiguration)
/// - Feature missing or disabled → `Error::forbidden`
pub fn require_feature(name: &str) -> RequireFeatureLayer {
    RequireFeatureLayer {
        name: name.to_owned(),
    }
}

/// Tower layer produced by [`require_feature()`].
pub struct RequireFeatureLayer {
    name: String,
}

impl Clone for RequireFeatureLayer {
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
        }
    }
}

impl<S> Layer<S> for RequireFeatureLayer {
    type Service = RequireFeatureService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        RequireFeatureService {
            inner,
            name: self.name.clone(),
        }
    }
}

/// Tower service produced by [`RequireFeatureLayer`].
pub struct RequireFeatureService<S> {
    inner: S,
    name: String,
}

impl<S: Clone> Clone for RequireFeatureService<S> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            name: self.name.clone(),
        }
    }
}

impl<S> Service<Request<Body>> for RequireFeatureService<S>
where
    S: Service<Request<Body>, Response = http::Response<Body>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Into<Box<dyn std::error::Error + Send + Sync>> + Send + 'static,
{
    type Response = http::Response<Body>;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request<Body>) -> Self::Future {
        let name = self.name.clone();
        let mut inner = self.inner.clone();
        std::mem::swap(&mut self.inner, &mut inner);

        Box::pin(async move {
            let Some(tier) = request.extensions().get::<TierInfo>() else {
                return Ok(
                    Error::internal("require_feature() called without TierLayer").into_response()
                );
            };

            if !tier.has_feature(&name) {
                return Ok(Error::forbidden(format!(
                    "Feature '{name}' is not available on your current plan"
                ))
                .into_response());
            }

            inner.call(request).await
        })
    }
}

// ---------------------------------------------------------------------------
// require_limit
// ---------------------------------------------------------------------------

/// Creates a guard layer that rejects requests when current usage meets or
/// exceeds the tier's limit for the named feature.
///
/// The `usage` closure receives `&Parts` and returns the current usage count.
///
/// Apply with `.route_layer()`. [`TierLayer`](super::TierLayer) must be
/// applied with `.layer()` upstream.
///
/// - `TierInfo` missing → `Error::internal`
/// - Feature not a `Limit` → `Error::internal`
/// - Usage >= limit → `Error::forbidden`
pub fn require_limit<F, Fut>(name: &str, usage: F) -> RequireLimitLayer<F>
where
    F: Fn(&Parts) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = crate::Result<u64>> + Send,
{
    RequireLimitLayer {
        name: name.to_owned(),
        usage,
    }
}

/// Tower layer produced by [`require_limit()`].
pub struct RequireLimitLayer<F> {
    name: String,
    usage: F,
}

impl<F: Clone> Clone for RequireLimitLayer<F> {
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
            usage: self.usage.clone(),
        }
    }
}

impl<S, F, Fut> Layer<S> for RequireLimitLayer<F>
where
    F: Fn(&Parts) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = crate::Result<u64>> + Send,
{
    type Service = RequireLimitService<S, F>;

    fn layer(&self, inner: S) -> Self::Service {
        RequireLimitService {
            inner,
            name: self.name.clone(),
            usage: self.usage.clone(),
        }
    }
}

/// Tower service produced by [`RequireLimitLayer`].
pub struct RequireLimitService<S, F> {
    inner: S,
    name: String,
    usage: F,
}

impl<S: Clone, F: Clone> Clone for RequireLimitService<S, F> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            name: self.name.clone(),
            usage: self.usage.clone(),
        }
    }
}

impl<S, F, Fut> Service<Request<Body>> for RequireLimitService<S, F>
where
    S: Service<Request<Body>, Response = http::Response<Body>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Into<Box<dyn std::error::Error + Send + Sync>> + Send + 'static,
    F: Fn(&Parts) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = crate::Result<u64>> + Send,
{
    type Response = http::Response<Body>;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request<Body>) -> Self::Future {
        let name = self.name.clone();
        let usage_fn = self.usage.clone();
        let mut inner = self.inner.clone();
        std::mem::swap(&mut self.inner, &mut inner);

        Box::pin(async move {
            let (parts, body) = request.into_parts();

            let Some(tier) = parts.extensions.get::<TierInfo>() else {
                return Ok(
                    Error::internal("require_limit() called without TierLayer").into_response()
                );
            };

            let ceiling = match tier.features.get(&name) {
                Some(FeatureAccess::Limit(v)) => *v,
                Some(FeatureAccess::Toggle(_)) => {
                    return Ok(
                        Error::internal(format!("Feature '{name}' is not a limit")).into_response()
                    );
                }
                None => {
                    return Ok(Error::forbidden(format!(
                        "Feature '{name}' is not available on your current plan"
                    ))
                    .into_response());
                }
            };

            let current = match (usage_fn)(&parts).await {
                Ok(v) => v,
                Err(e) => return Ok(e.into_response()),
            };

            if current >= ceiling {
                return Ok(Error::forbidden(format!(
                    "Limit exceeded for '{name}': {current}/{ceiling}"
                ))
                .into_response());
            }

            let request = Request::from_parts(parts, body);
            inner.call(request).await
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::convert::Infallible;

    use http::{Response, StatusCode};
    use tower::ServiceExt;

    use super::super::types::FeatureAccess;

    fn tier_with(features: HashMap<String, FeatureAccess>) -> TierInfo {
        TierInfo {
            name: "test".into(),
            features,
        }
    }

    async fn ok_handler(_req: Request<Body>) -> Result<Response<Body>, Infallible> {
        Ok(Response::new(Body::from("ok")))
    }

    // --- require_feature ---

    #[tokio::test]
    async fn feature_passes_when_toggle_true() {
        let layer = require_feature("sso");
        let svc = layer.layer(tower::service_fn(ok_handler));

        let mut req = Request::builder().body(Body::empty()).unwrap();
        req.extensions_mut().insert(tier_with(HashMap::from([
            ("sso".into(), FeatureAccess::Toggle(true)),
        ])));
        let resp = svc.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn feature_passes_when_limit_positive() {
        let layer = require_feature("api_calls");
        let svc = layer.layer(tower::service_fn(ok_handler));

        let mut req = Request::builder().body(Body::empty()).unwrap();
        req.extensions_mut().insert(tier_with(HashMap::from([
            ("api_calls".into(), FeatureAccess::Limit(1_000)),
        ])));
        let resp = svc.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn feature_403_when_toggle_false() {
        let layer = require_feature("sso");
        let svc = layer.layer(tower::service_fn(ok_handler));

        let mut req = Request::builder().body(Body::empty()).unwrap();
        req.extensions_mut().insert(tier_with(HashMap::from([
            ("sso".into(), FeatureAccess::Toggle(false)),
        ])));
        let resp = svc.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn feature_403_when_missing() {
        let layer = require_feature("sso");
        let svc = layer.layer(tower::service_fn(ok_handler));

        let mut req = Request::builder().body(Body::empty()).unwrap();
        req.extensions_mut().insert(tier_with(HashMap::new()));
        let resp = svc.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn feature_500_when_no_tier_info() {
        let layer = require_feature("sso");
        let svc = layer.layer(tower::service_fn(ok_handler));

        let req = Request::builder().body(Body::empty()).unwrap();
        let resp = svc.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn feature_does_not_call_inner_on_reject() {
        use std::sync::atomic::{AtomicBool, Ordering};

        let called = Arc::new(AtomicBool::new(false));
        let called_clone = called.clone();

        let layer = require_feature("sso");
        let svc = layer.layer(tower::service_fn(move |_req: Request<Body>| {
            let called = called_clone.clone();
            async move {
                called.store(true, Ordering::SeqCst);
                Ok::<_, Infallible>(Response::new(Body::from("should not reach")))
            }
        }));

        let mut req = Request::builder().body(Body::empty()).unwrap();
        req.extensions_mut().insert(tier_with(HashMap::from([
            ("sso".into(), FeatureAccess::Toggle(false)),
        ])));
        let resp = svc.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        assert!(!called.load(Ordering::SeqCst));
    }

    // --- require_limit ---

    #[tokio::test]
    async fn limit_passes_when_under() {
        let layer = require_limit("api_calls", |_parts| async { Ok(500u64) });
        let svc = layer.layer(tower::service_fn(ok_handler));

        let mut req = Request::builder().body(Body::empty()).unwrap();
        req.extensions_mut().insert(tier_with(HashMap::from([
            ("api_calls".into(), FeatureAccess::Limit(1_000)),
        ])));
        let resp = svc.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn limit_403_when_at_ceiling() {
        let layer = require_limit("api_calls", |_parts| async { Ok(1_000u64) });
        let svc = layer.layer(tower::service_fn(ok_handler));

        let mut req = Request::builder().body(Body::empty()).unwrap();
        req.extensions_mut().insert(tier_with(HashMap::from([
            ("api_calls".into(), FeatureAccess::Limit(1_000)),
        ])));
        let resp = svc.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn limit_403_when_over() {
        let layer = require_limit("api_calls", |_parts| async { Ok(2_000u64) });
        let svc = layer.layer(tower::service_fn(ok_handler));

        let mut req = Request::builder().body(Body::empty()).unwrap();
        req.extensions_mut().insert(tier_with(HashMap::from([
            ("api_calls".into(), FeatureAccess::Limit(1_000)),
        ])));
        let resp = svc.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn limit_500_when_feature_is_toggle() {
        let layer = require_limit("sso", |_parts| async { Ok(0u64) });
        let svc = layer.layer(tower::service_fn(ok_handler));

        let mut req = Request::builder().body(Body::empty()).unwrap();
        req.extensions_mut().insert(tier_with(HashMap::from([
            ("sso".into(), FeatureAccess::Toggle(true)),
        ])));
        let resp = svc.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn limit_403_when_feature_missing() {
        let layer = require_limit("api_calls", |_parts| async { Ok(0u64) });
        let svc = layer.layer(tower::service_fn(ok_handler));

        let mut req = Request::builder().body(Body::empty()).unwrap();
        req.extensions_mut().insert(tier_with(HashMap::new()));
        let resp = svc.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn limit_500_when_no_tier_info() {
        let layer = require_limit("api_calls", |_parts| async { Ok(0u64) });
        let svc = layer.layer(tower::service_fn(ok_handler));

        let req = Request::builder().body(Body::empty()).unwrap();
        let resp = svc.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn limit_usage_closure_error_returns_error() {
        let layer = require_limit("api_calls", |_parts| async {
            Err::<u64, _>(Error::internal("db is down"))
        });
        let svc = layer.layer(tower::service_fn(ok_handler));

        let mut req = Request::builder().body(Body::empty()).unwrap();
        req.extensions_mut().insert(tier_with(HashMap::from([
            ("api_calls".into(), FeatureAccess::Limit(1_000)),
        ])));
        let resp = svc.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn limit_does_not_call_inner_on_reject() {
        use std::sync::atomic::{AtomicBool, Ordering};

        let called = Arc::new(AtomicBool::new(false));
        let called_clone = called.clone();

        let layer = require_limit("api_calls", |_parts| async { Ok(2_000u64) });
        let svc = layer.layer(tower::service_fn(move |_req: Request<Body>| {
            let called = called_clone.clone();
            async move {
                called.store(true, Ordering::SeqCst);
                Ok::<_, Infallible>(Response::new(Body::from("should not reach")))
            }
        }));

        let mut req = Request::builder().body(Body::empty()).unwrap();
        req.extensions_mut().insert(tier_with(HashMap::from([
            ("api_calls".into(), FeatureAccess::Limit(1_000)),
        ])));
        let resp = svc.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        assert!(!called.load(Ordering::SeqCst));
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test --features tier tier::guard`
Expected: all 14 tests pass

- [ ] **Step 3: Commit**

```bash
git add src/tier/guard.rs
git commit -m "feat(tier): implement require_feature and require_limit guards"
```

---

## Task 6: Test Backend & Re-exports

**Files:**
- Modify: `src/tier/types.rs` (add test backend)
- Modify: `src/tier/mod.rs` (add test module re-export)
- Modify: `src/lib.rs` (add tier re-exports)

- [ ] **Step 1: Add in-memory test backend to `src/tier/types.rs`**

Add at the bottom of `src/tier/types.rs`, before the `#[cfg(test)]` block:

```rust
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
```

- [ ] **Step 2: Add test module re-export to `src/tier/mod.rs`**

Add after the existing `pub use` lines:

```rust
/// Test helpers for the tier module.
///
/// Available when running tests or when the `test-helpers` feature is enabled.
#[cfg_attr(not(any(test, feature = "test-helpers")), allow(dead_code))]
pub mod test {
    pub use super::types::test_support::{FailingTierBackend, StaticTierBackend};
}
```

- [ ] **Step 3: Add re-exports to `src/lib.rs`**

Add after the `apikey` re-exports block:

```rust
#[cfg(feature = "tier")]
pub use tier::{
    FeatureAccess, TierBackend, TierInfo, TierLayer, TierResolver, require_feature, require_limit,
};
```

- [ ] **Step 4: Verify everything compiles and tests pass**

Run: `cargo test --features tier`
Expected: all tier tests pass

Run: `cargo check --features full`
Expected: compiles with no errors

- [ ] **Step 5: Commit**

```bash
git add src/tier/types.rs src/tier/mod.rs src/lib.rs
git commit -m "feat(tier): add test backends and lib.rs re-exports"
```

---

## Task 7: Template Integration

**Files:**
- Modify: `src/template/middleware.rs`

- [ ] **Step 1: Add tier context injection**

In `src/template/middleware.rs`, inside the `call` method of `TemplateContextMiddleware`, after the flash_messages block (around line 153, before `parts.extensions.insert(ctx)`), add:

```rust
// tier info (if tier feature enabled and TierInfo in extensions)
#[cfg(feature = "tier")]
if let Some(tier_info) = parts.extensions.get::<crate::tier::TierInfo>() {
    ctx.set(
        "tier_name",
        minijinja::Value::from(tier_info.name.clone()),
    );

    let ti = tier_info.clone();
    ctx.set(
        "tier_has",
        minijinja::Value::from_function(
            move |name: &str| -> bool { ti.has_feature(name) },
        ),
    );

    let ti = tier_info.clone();
    ctx.set(
        "tier_enabled",
        minijinja::Value::from_function(
            move |name: &str| -> bool { ti.is_enabled(name) },
        ),
    );

    let ti = tier_info.clone();
    ctx.set(
        "tier_limit",
        minijinja::Value::from_function(
            move |name: &str| -> Option<u64> { ti.limit(name) },
        ),
    );
}
```

- [ ] **Step 2: Add tests for template integration**

Add to the `#[cfg(test)] mod tests` block at the bottom of `src/template/middleware.rs`:

```rust
#[cfg(feature = "tier")]
mod tier_tests {
    use super::*;
    use std::collections::HashMap;

    use crate::tier::{FeatureAccess, TierInfo};

    fn test_tier() -> TierInfo {
        TierInfo {
            name: "pro".into(),
            features: HashMap::from([
                ("sso".into(), FeatureAccess::Toggle(true)),
                ("custom_domain".into(), FeatureAccess::Toggle(false)),
                ("api_calls".into(), FeatureAccess::Limit(100_000)),
            ]),
        }
    }

    async fn extract_tier_name(req: Request<Body>) -> (StatusCode, String) {
        let ctx = req.extensions().get::<TemplateContext>().unwrap();
        let name = ctx
            .get("tier_name")
            .map(|v| v.to_string())
            .unwrap_or_default();
        (StatusCode::OK, name)
    }

    #[tokio::test]
    async fn injects_tier_name() {
        let (_dir, engine) = test_engine();
        let app = Router::new()
            .route("/test", get(extract_tier_name))
            .layer(TemplateContextLayer::new(engine));

        let mut req = Request::builder().uri("/test").body(Body::empty()).unwrap();
        req.extensions_mut().insert(test_tier());
        let resp = app.oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(body, "pro");
    }

    #[tokio::test]
    async fn tier_has_function_works() {
        let (_dir, engine) = test_engine();
        let tpl_dir = _dir.path().join("templates");
        std::fs::write(
            tpl_dir.join("tier_has_test.html"),
            "{% if tier_has('sso') %}yes{% else %}no{% endif %}",
        )
        .unwrap();

        let mut ctx = TemplateContext::default();
        let tier = test_tier();
        ctx.set("tier_name", minijinja::Value::from(tier.name.clone()));

        let ti = tier.clone();
        ctx.set(
            "tier_has",
            minijinja::Value::from_function(move |name: &str| -> bool {
                ti.has_feature(name)
            }),
        );

        let merged = ctx.merge(minijinja::context! {});
        let result = engine.render("tier_has_test.html", merged).unwrap();
        assert_eq!(result, "yes");
    }

    #[tokio::test]
    async fn tier_has_returns_false_for_disabled() {
        let (_dir, engine) = test_engine();
        let tpl_dir = _dir.path().join("templates");
        std::fs::write(
            tpl_dir.join("tier_disabled_test.html"),
            "{% if tier_has('custom_domain') %}yes{% else %}no{% endif %}",
        )
        .unwrap();

        let mut ctx = TemplateContext::default();
        let tier = test_tier();

        let ti = tier.clone();
        ctx.set(
            "tier_has",
            minijinja::Value::from_function(move |name: &str| -> bool {
                ti.has_feature(name)
            }),
        );

        let merged = ctx.merge(minijinja::context! {});
        let result = engine.render("tier_disabled_test.html", merged).unwrap();
        assert_eq!(result, "no");
    }

    #[tokio::test]
    async fn tier_limit_function_works() {
        let (_dir, engine) = test_engine();
        let tpl_dir = _dir.path().join("templates");
        std::fs::write(
            tpl_dir.join("tier_limit_test.html"),
            "{{ tier_limit('api_calls') }}",
        )
        .unwrap();

        let mut ctx = TemplateContext::default();
        let tier = test_tier();

        let ti = tier.clone();
        ctx.set(
            "tier_limit",
            minijinja::Value::from_function(move |name: &str| -> Option<u64> {
                ti.limit(name)
            }),
        );

        let merged = ctx.merge(minijinja::context! {});
        let result = engine.render("tier_limit_test.html", merged).unwrap();
        assert_eq!(result, "100000");
    }

    #[tokio::test]
    async fn no_tier_info_no_injection() {
        let (_dir, engine) = test_engine();
        let app = Router::new()
            .route("/test", get(extract_tier_name))
            .layer(TemplateContextLayer::new(engine));

        let req = Request::builder().uri("/test").body(Body::empty()).unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        // tier_name not set — returns empty string
        assert_eq!(body, "");
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test --features tier,templates template::middleware::tests::tier_tests`
Expected: all 4 tier template tests pass

Run: `cargo test --features tier,templates template::middleware`
Expected: all existing template tests still pass (no regressions)

- [ ] **Step 4: Commit**

```bash
git add src/template/middleware.rs
git commit -m "feat(tier): inject tier functions into template context"
```

---

## Task 8: Full Test Suite & Clippy

**Files:** None (verification only)

- [ ] **Step 1: Run full tier test suite**

Run: `cargo test --features tier`
Expected: all tier module tests pass

- [ ] **Step 2: Run tier + templates tests**

Run: `cargo test --features tier,templates`
Expected: all tests pass including template integration

- [ ] **Step 3: Run clippy**

Run: `cargo clippy --features tier --tests -- -D warnings`
Expected: no warnings

Run: `cargo clippy --features tier,templates --tests -- -D warnings`
Expected: no warnings

- [ ] **Step 4: Run full feature set check**

Run: `cargo clippy --features full --tests -- -D warnings`
Expected: no warnings (tier included in full)

- [ ] **Step 5: Format check**

Run: `cargo fmt --check`
Expected: no formatting issues

- [ ] **Step 6: Commit any fixes if needed, then final commit**

If clippy or fmt required fixes:
```bash
git add -A
git commit -m "fix(tier): address clippy and formatting issues"
```
