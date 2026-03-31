use std::future::Future;
use std::pin::Pin;
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
                    Error::internal("require_feature() called without TierLayer").into_response(),
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
    use std::sync::Arc;

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
        req.extensions_mut().insert(tier_with(HashMap::from([(
            "sso".into(),
            FeatureAccess::Toggle(true),
        )])));
        let resp = svc.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn feature_passes_when_limit_positive() {
        let layer = require_feature("api_calls");
        let svc = layer.layer(tower::service_fn(ok_handler));

        let mut req = Request::builder().body(Body::empty()).unwrap();
        req.extensions_mut().insert(tier_with(HashMap::from([(
            "api_calls".into(),
            FeatureAccess::Limit(1_000),
        )])));
        let resp = svc.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn feature_403_when_toggle_false() {
        let layer = require_feature("sso");
        let svc = layer.layer(tower::service_fn(ok_handler));

        let mut req = Request::builder().body(Body::empty()).unwrap();
        req.extensions_mut().insert(tier_with(HashMap::from([(
            "sso".into(),
            FeatureAccess::Toggle(false),
        )])));
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
        req.extensions_mut().insert(tier_with(HashMap::from([(
            "sso".into(),
            FeatureAccess::Toggle(false),
        )])));
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
        req.extensions_mut().insert(tier_with(HashMap::from([(
            "api_calls".into(),
            FeatureAccess::Limit(1_000),
        )])));
        let resp = svc.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn limit_403_when_at_ceiling() {
        let layer = require_limit("api_calls", |_parts| async { Ok(1_000u64) });
        let svc = layer.layer(tower::service_fn(ok_handler));

        let mut req = Request::builder().body(Body::empty()).unwrap();
        req.extensions_mut().insert(tier_with(HashMap::from([(
            "api_calls".into(),
            FeatureAccess::Limit(1_000),
        )])));
        let resp = svc.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn limit_403_when_over() {
        let layer = require_limit("api_calls", |_parts| async { Ok(2_000u64) });
        let svc = layer.layer(tower::service_fn(ok_handler));

        let mut req = Request::builder().body(Body::empty()).unwrap();
        req.extensions_mut().insert(tier_with(HashMap::from([(
            "api_calls".into(),
            FeatureAccess::Limit(1_000),
        )])));
        let resp = svc.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn limit_500_when_feature_is_toggle() {
        let layer = require_limit("sso", |_parts| async { Ok(0u64) });
        let svc = layer.layer(tower::service_fn(ok_handler));

        let mut req = Request::builder().body(Body::empty()).unwrap();
        req.extensions_mut().insert(tier_with(HashMap::from([(
            "sso".into(),
            FeatureAccess::Toggle(true),
        )])));
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
        req.extensions_mut().insert(tier_with(HashMap::from([(
            "api_calls".into(),
            FeatureAccess::Limit(1_000),
        )])));
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
        req.extensions_mut().insert(tier_with(HashMap::from([(
            "api_calls".into(),
            FeatureAccess::Limit(1_000),
        )])));
        let resp = svc.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        assert!(!called.load(Ordering::SeqCst));
    }
}
