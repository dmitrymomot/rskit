use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use axum::body::Body;
use axum::response::IntoResponse;
use http::Request;
use tower::{Layer, Service};

use super::traits::{HasTenantId, TenantResolver, TenantStrategy};

/// Creates a tenant middleware layer from a strategy and resolver.
pub fn middleware<S, R>(strategy: S, resolver: R) -> TenantLayer<S, R>
where
    S: TenantStrategy,
    R: TenantResolver,
{
    TenantLayer::new(strategy, resolver)
}

/// Tower layer that produces `TenantMiddleware` services.
pub struct TenantLayer<S, R> {
    strategy: Arc<S>,
    resolver: Arc<R>,
}

impl<S, R> Clone for TenantLayer<S, R> {
    fn clone(&self) -> Self {
        Self {
            strategy: self.strategy.clone(),
            resolver: self.resolver.clone(),
        }
    }
}

impl<S, R> TenantLayer<S, R> {
    pub fn new(strategy: S, resolver: R) -> Self {
        Self {
            strategy: Arc::new(strategy),
            resolver: Arc::new(resolver),
        }
    }
}

impl<Svc, S, R> Layer<Svc> for TenantLayer<S, R>
where
    S: TenantStrategy,
    R: TenantResolver,
{
    type Service = TenantMiddleware<Svc, S, R>;

    fn layer(&self, inner: Svc) -> Self::Service {
        TenantMiddleware {
            inner,
            strategy: self.strategy.clone(),
            resolver: self.resolver.clone(),
        }
    }
}

/// Tower service that resolves tenants from requests.
pub struct TenantMiddleware<Svc, S, R> {
    inner: Svc,
    strategy: Arc<S>,
    resolver: Arc<R>,
}

impl<Svc: Clone, S, R> Clone for TenantMiddleware<Svc, S, R> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            strategy: self.strategy.clone(),
            resolver: self.resolver.clone(),
        }
    }
}

impl<Svc, S, R> Service<Request<Body>> for TenantMiddleware<Svc, S, R>
where
    Svc: Service<Request<Body>, Response = http::Response<Body>> + Clone + Send + 'static,
    Svc::Future: Send + 'static,
    Svc::Error: Into<Box<dyn std::error::Error + Send + Sync>> + Send + 'static,
    S: TenantStrategy,
    R: TenantResolver,
{
    type Response = http::Response<Body>;
    type Error = Svc::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request<Body>) -> Self::Future {
        let strategy = self.strategy.clone();
        let resolver = self.resolver.clone();
        let mut inner = self.inner.clone();
        std::mem::swap(&mut self.inner, &mut inner);

        Box::pin(async move {
            let (mut parts, body) = request.into_parts();

            // Step 1: Extract tenant identifier
            let tenant_id = match strategy.extract(&mut parts) {
                Ok(id) => id,
                Err(e) => return Ok(e.into_response()),
            };

            // Step 2: Resolve tenant
            let tenant = match resolver.resolve(&tenant_id).await {
                Ok(t) => t,
                Err(e) => return Ok(e.into_response()),
            };

            // Step 3: Record tenant_id in tracing span
            tracing::Span::current().record("tenant_id", tenant.tenant_id());

            // Step 4: Insert into extensions
            let tenant = Arc::new(tenant);
            parts.extensions.insert(tenant);

            let request = Request::from_parts(parts, body);
            inner.call(request).await
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use http::{Request, Response, StatusCode};
    use std::convert::Infallible;
    use tower::ServiceExt;

    use crate::error::Error;
    use crate::tenant::TenantId;

    #[derive(Clone, Debug)]
    struct TestTenant {
        id: String,
    }

    impl HasTenantId for TestTenant {
        fn tenant_id(&self) -> &str {
            &self.id
        }
    }

    struct OkStrategy;
    impl TenantStrategy for OkStrategy {
        fn extract(&self, _parts: &mut http::request::Parts) -> crate::Result<TenantId> {
            Ok(TenantId::Slug("acme".into()))
        }
    }

    struct FailStrategy;
    impl TenantStrategy for FailStrategy {
        fn extract(&self, _parts: &mut http::request::Parts) -> crate::Result<TenantId> {
            Err(Error::bad_request("no tenant"))
        }
    }

    struct OkResolver;
    impl TenantResolver for OkResolver {
        type Tenant = TestTenant;
        async fn resolve(&self, _id: &TenantId) -> crate::Result<TestTenant> {
            Ok(TestTenant { id: "t1".into() })
        }
    }

    struct NotFoundResolver;
    impl TenantResolver for NotFoundResolver {
        type Tenant = TestTenant;
        async fn resolve(&self, _id: &TenantId) -> crate::Result<TestTenant> {
            Err(Error::not_found("tenant not found"))
        }
    }

    struct InternalErrorResolver;
    impl TenantResolver for InternalErrorResolver {
        type Tenant = TestTenant;
        async fn resolve(&self, _id: &TenantId) -> crate::Result<TestTenant> {
            Err(Error::internal("db failure"))
        }
    }

    /// Inner service that checks extensions for resolved tenant.
    async fn echo_handler(req: Request<Body>) -> Result<Response<Body>, Infallible> {
        let has_tenant = req.extensions().get::<Arc<TestTenant>>().is_some();
        let body = if has_tenant { "ok" } else { "no-tenant" };
        Ok(Response::new(Body::from(body)))
    }

    #[tokio::test]
    async fn strategy_ok_resolver_ok_passes_through() {
        let layer = TenantLayer::new(OkStrategy, OkResolver);
        let svc = layer.layer(tower::service_fn(echo_handler));

        let req = Request::builder().body(Body::empty()).unwrap();
        let resp = svc.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn strategy_fail_returns_400() {
        let layer = TenantLayer::new(FailStrategy, OkResolver);
        let svc = layer.layer(tower::service_fn(echo_handler));

        let req = Request::builder().body(Body::empty()).unwrap();
        let resp = svc.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn resolver_not_found_returns_404() {
        let layer = TenantLayer::new(OkStrategy, NotFoundResolver);
        let svc = layer.layer(tower::service_fn(echo_handler));

        let req = Request::builder().body(Body::empty()).unwrap();
        let resp = svc.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn resolver_internal_error_returns_500() {
        let layer = TenantLayer::new(OkStrategy, InternalErrorResolver);
        let svc = layer.layer(tower::service_fn(echo_handler));

        let req = Request::builder().body(Body::empty()).unwrap();
        let resp = svc.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn strategy_fail_does_not_call_inner() {
        use std::sync::atomic::{AtomicBool, Ordering};

        let called = Arc::new(AtomicBool::new(false));
        let called_clone = called.clone();

        let layer = TenantLayer::new(FailStrategy, OkResolver);
        let svc = layer.layer(tower::service_fn(move |_req: Request<Body>| {
            let called = called_clone.clone();
            async move {
                called.store(true, Ordering::SeqCst);
                Ok::<_, Infallible>(Response::new(Body::from("should not reach")))
            }
        }));

        let req = Request::builder().body(Body::empty()).unwrap();
        let resp = svc.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        assert!(!called.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn resolver_fail_does_not_call_inner() {
        use std::sync::atomic::{AtomicBool, Ordering};

        let called = Arc::new(AtomicBool::new(false));
        let called_clone = called.clone();

        let layer = TenantLayer::new(OkStrategy, NotFoundResolver);
        let svc = layer.layer(tower::service_fn(move |_req: Request<Body>| {
            let called = called_clone.clone();
            async move {
                called.store(true, Ordering::SeqCst);
                Ok::<_, Infallible>(Response::new(Body::from("should not reach")))
            }
        }));

        let req = Request::builder().body(Body::empty()).unwrap();
        let resp = svc.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        assert!(!called.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn tenant_in_extensions_after_resolve() {
        let layer = TenantLayer::new(OkStrategy, OkResolver);

        // Custom inner service that asserts tenant is in extensions
        let inner = tower::service_fn(|req: Request<Body>| async move {
            let tenant = req.extensions().get::<Arc<TestTenant>>().unwrap();
            assert_eq!(tenant.id, "t1");
            Ok::<_, Infallible>(Response::new(Body::empty()))
        });

        let svc = layer.layer(inner);
        let req = Request::builder().body(Body::empty()).unwrap();
        let resp = svc.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }
}
