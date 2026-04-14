use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use axum::body::Body;
use axum::response::IntoResponse;
use http::Request;
use tower::{Layer, Service};

use super::extractor::Role;
use super::traits::RoleExtractor;

/// Creates an RBAC middleware layer from a role extractor.
///
/// Calls `extractor.extract()` on every request, stores the resulting [`Role`] in
/// request extensions, then forwards to the inner service. Extraction errors are
/// converted to HTTP responses immediately; the inner service is not called.
///
/// Apply with `.layer()` on the outer router so the role is available to any
/// `.route_layer()` guards that run after route matching.
pub fn middleware<R>(extractor: R) -> RbacLayer<R>
where
    R: RoleExtractor,
{
    RbacLayer {
        extractor: Arc::new(extractor),
    }
}

/// Tower layer produced by [`middleware()`].
pub struct RbacLayer<R> {
    extractor: Arc<R>,
}

impl<R> Clone for RbacLayer<R> {
    fn clone(&self) -> Self {
        Self {
            extractor: self.extractor.clone(),
        }
    }
}

impl<Svc, R> Layer<Svc> for RbacLayer<R>
where
    R: RoleExtractor,
{
    type Service = RbacMiddleware<Svc, R>;

    fn layer(&self, inner: Svc) -> Self::Service {
        RbacMiddleware {
            inner,
            extractor: self.extractor.clone(),
        }
    }
}

/// Tower service produced by [`RbacLayer`].
pub struct RbacMiddleware<Svc, R> {
    inner: Svc,
    extractor: Arc<R>,
}

impl<Svc: Clone, R> Clone for RbacMiddleware<Svc, R> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            extractor: self.extractor.clone(),
        }
    }
}

impl<Svc, R> Service<Request<Body>> for RbacMiddleware<Svc, R>
where
    Svc: Service<Request<Body>, Response = http::Response<Body>> + Clone + Send + 'static,
    Svc::Future: Send + 'static,
    Svc::Error: Into<Box<dyn std::error::Error + Send + Sync>> + Send + 'static,
    R: RoleExtractor,
{
    type Response = http::Response<Body>;
    type Error = Svc::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request<Body>) -> Self::Future {
        let extractor = self.extractor.clone();
        let mut inner = self.inner.clone();
        std::mem::swap(&mut self.inner, &mut inner);

        Box::pin(async move {
            let (mut parts, body) = request.into_parts();

            // Extract role
            let role = match extractor.extract(&mut parts).await {
                Ok(r) => r,
                Err(e) => return Ok(e.into_response()),
            };

            // Insert into extensions
            parts.extensions.insert(Role(role));

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

    struct OkExtractor;
    impl RoleExtractor for OkExtractor {
        async fn extract(&self, _parts: &mut http::request::Parts) -> crate::Result<String> {
            Ok("admin".into())
        }
    }

    struct UnauthorizedExtractor;
    impl RoleExtractor for UnauthorizedExtractor {
        async fn extract(&self, _parts: &mut http::request::Parts) -> crate::Result<String> {
            Err(Error::unauthorized("not authenticated"))
        }
    }

    struct InternalErrorExtractor;
    impl RoleExtractor for InternalErrorExtractor {
        async fn extract(&self, _parts: &mut http::request::Parts) -> crate::Result<String> {
            Err(Error::internal("db failure"))
        }
    }

    async fn echo_handler(req: Request<Body>) -> Result<Response<Body>, Infallible> {
        let has_role = req.extensions().get::<Role>().is_some();
        let body = if has_role { "ok" } else { "no-role" };
        Ok(Response::new(Body::from(body)))
    }

    #[tokio::test]
    async fn extractor_ok_passes_through() {
        let layer = middleware(OkExtractor);
        let svc = layer.layer(tower::service_fn(echo_handler));

        let req = Request::builder().body(Body::empty()).unwrap();
        let resp = svc.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn extractor_unauthorized_returns_401() {
        let layer = middleware(UnauthorizedExtractor);
        let svc = layer.layer(tower::service_fn(echo_handler));

        let req = Request::builder().body(Body::empty()).unwrap();
        let resp = svc.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn extractor_internal_error_returns_500() {
        let layer = middleware(InternalErrorExtractor);
        let svc = layer.layer(tower::service_fn(echo_handler));

        let req = Request::builder().body(Body::empty()).unwrap();
        let resp = svc.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn extractor_fail_does_not_call_inner() {
        use std::sync::atomic::{AtomicBool, Ordering};

        let called = Arc::new(AtomicBool::new(false));
        let called_clone = called.clone();

        let layer = middleware(UnauthorizedExtractor);
        let svc = layer.layer(tower::service_fn(move |_req: Request<Body>| {
            let called = called_clone.clone();
            async move {
                called.store(true, Ordering::SeqCst);
                Ok::<_, Infallible>(Response::new(Body::from("should not reach")))
            }
        }));

        let req = Request::builder().body(Body::empty()).unwrap();
        let resp = svc.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
        assert!(!called.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn role_in_extensions_after_extract() {
        let layer = middleware(OkExtractor);

        let inner = tower::service_fn(|req: Request<Body>| async move {
            let role = req.extensions().get::<Role>().unwrap();
            assert_eq!(role.as_str(), "admin");
            Ok::<_, Infallible>(Response::new(Body::empty()))
        });

        let svc = layer.layer(inner);
        let req = Request::builder().body(Body::empty()).unwrap();
        let resp = svc.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }
}
