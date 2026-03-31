use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use axum::body::Body;
use axum::response::IntoResponse;
use http::Request;
use http::request::Parts;
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

    use super::super::types::{FeatureAccess, TierBackend};
    use crate::error::Error;

    fn pro_tier() -> TierInfo {
        TierInfo {
            name: "pro".into(),
            features: HashMap::from([("sso".into(), FeatureAccess::Toggle(true))]),
        }
    }

    fn anon_tier() -> TierInfo {
        TierInfo {
            name: "anonymous".into(),
            features: HashMap::from([("public_api".into(), FeatureAccess::Toggle(true))]),
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

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(body, "tier-present");
    }

    #[tokio::test]
    async fn extractor_none_no_default_skips() {
        let layer = TierLayer::new(resolver(pro_tier()), |_| None);
        let svc = layer.layer(tower::service_fn(ok_handler));

        let req = Request::builder().body(Body::empty()).unwrap();
        let resp = svc.oneshot(req).await.unwrap();

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(body, "no-tier");
    }

    #[tokio::test]
    async fn extractor_none_with_default_injects_default() {
        let layer = TierLayer::new(resolver(pro_tier()), |_| None).with_default(anon_tier());
        let svc = layer.layer(tower::service_fn(ok_handler));

        let req = Request::builder().body(Body::empty()).unwrap();
        let resp = svc.oneshot(req).await.unwrap();

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
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

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(body, "tier-present");
    }
}
