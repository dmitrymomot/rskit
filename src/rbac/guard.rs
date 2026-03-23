use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use axum::body::Body;
use axum::response::IntoResponse;
use http::Request;
use tower::{Layer, Service};

use crate::Error;

use super::extractor::Role;

// --- require_role ---

/// Creates a guard layer that rejects requests unless the resolved role
/// matches ANY of the allowed roles. Returns 403 Forbidden if role is
/// present but not allowed, 401 Unauthorized if no role is present.
pub fn require_role(roles: impl IntoIterator<Item = impl Into<String>>) -> RequireRoleLayer {
    RequireRoleLayer {
        roles: Arc::new(roles.into_iter().map(Into::into).collect()),
    }
}

pub struct RequireRoleLayer {
    roles: Arc<Vec<String>>,
}

impl Clone for RequireRoleLayer {
    fn clone(&self) -> Self {
        Self {
            roles: self.roles.clone(),
        }
    }
}

impl<S> Layer<S> for RequireRoleLayer {
    type Service = RequireRoleService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        RequireRoleService {
            inner,
            roles: self.roles.clone(),
        }
    }
}

pub struct RequireRoleService<S> {
    inner: S,
    roles: Arc<Vec<String>>,
}

impl<S: Clone> Clone for RequireRoleService<S> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            roles: self.roles.clone(),
        }
    }
}

impl<S> Service<Request<Body>> for RequireRoleService<S>
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
        let roles = self.roles.clone();
        let mut inner = self.inner.clone();
        std::mem::swap(&mut self.inner, &mut inner);

        Box::pin(async move {
            let role = match request.extensions().get::<Role>() {
                Some(r) => r,
                None => {
                    return Ok(Error::unauthorized("authentication required").into_response());
                }
            };

            if !roles.iter().any(|allowed| allowed == role.as_str()) {
                return Ok(Error::forbidden("insufficient role").into_response());
            }

            inner.call(request).await
        })
    }
}

// --- require_authenticated ---

/// Creates a guard layer that rejects requests unless a `Role` is present
/// in extensions. Returns 401 Unauthorized if no role is present.
pub fn require_authenticated() -> RequireAuthenticatedLayer {
    RequireAuthenticatedLayer
}

pub struct RequireAuthenticatedLayer;

impl Clone for RequireAuthenticatedLayer {
    fn clone(&self) -> Self {
        Self
    }
}

impl<S> Layer<S> for RequireAuthenticatedLayer {
    type Service = RequireAuthenticatedService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        RequireAuthenticatedService { inner }
    }
}

pub struct RequireAuthenticatedService<S> {
    inner: S,
}

impl<S: Clone> Clone for RequireAuthenticatedService<S> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<S> Service<Request<Body>> for RequireAuthenticatedService<S>
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
        let mut inner = self.inner.clone();
        std::mem::swap(&mut self.inner, &mut inner);

        Box::pin(async move {
            if request.extensions().get::<Role>().is_none() {
                return Ok(Error::unauthorized("authentication required").into_response());
            }

            inner.call(request).await
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::{Response, StatusCode};
    use std::convert::Infallible;
    use tower::ServiceExt;

    async fn ok_handler(_req: Request<Body>) -> Result<Response<Body>, Infallible> {
        Ok(Response::new(Body::from("ok")))
    }

    // --- require_role tests ---

    #[tokio::test]
    async fn require_role_passes_when_role_in_list() {
        let layer = require_role(["admin", "owner"]);
        let svc = layer.layer(tower::service_fn(ok_handler));

        let mut req = Request::builder().body(Body::empty()).unwrap();
        req.extensions_mut().insert(Role("admin".into()));
        let resp = svc.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn require_role_403_when_role_not_in_list() {
        let layer = require_role(["admin", "owner"]);
        let svc = layer.layer(tower::service_fn(ok_handler));

        let mut req = Request::builder().body(Body::empty()).unwrap();
        req.extensions_mut().insert(Role("viewer".into()));
        let resp = svc.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn require_role_401_when_role_missing() {
        let layer = require_role(["admin"]);
        let svc = layer.layer(tower::service_fn(ok_handler));

        let req = Request::builder().body(Body::empty()).unwrap();
        let resp = svc.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn require_role_403_when_empty_roles_list() {
        let layer = require_role(std::iter::empty::<String>());
        let svc = layer.layer(tower::service_fn(ok_handler));

        let mut req = Request::builder().body(Body::empty()).unwrap();
        req.extensions_mut().insert(Role("admin".into()));
        let resp = svc.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn require_role_empty_string_matches() {
        let layer = require_role([""]);
        let svc = layer.layer(tower::service_fn(ok_handler));

        let mut req = Request::builder().body(Body::empty()).unwrap();
        req.extensions_mut().insert(Role("".into()));
        let resp = svc.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn require_role_does_not_call_inner_on_reject() {
        use std::sync::atomic::{AtomicBool, Ordering};

        let called = Arc::new(AtomicBool::new(false));
        let called_clone = called.clone();

        let layer = require_role(["admin"]);
        let svc = layer.layer(tower::service_fn(move |_req: Request<Body>| {
            let called = called_clone.clone();
            async move {
                called.store(true, Ordering::SeqCst);
                Ok::<_, Infallible>(Response::new(Body::from("should not reach")))
            }
        }));

        let mut req = Request::builder().body(Body::empty()).unwrap();
        req.extensions_mut().insert(Role("viewer".into()));
        let resp = svc.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        assert!(!called.load(Ordering::SeqCst));
    }

    // --- require_authenticated tests ---

    #[tokio::test]
    async fn require_authenticated_passes_when_role_present() {
        let layer = require_authenticated();
        let svc = layer.layer(tower::service_fn(ok_handler));

        let mut req = Request::builder().body(Body::empty()).unwrap();
        req.extensions_mut().insert(Role("viewer".into()));
        let resp = svc.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn require_authenticated_401_when_role_missing() {
        let layer = require_authenticated();
        let svc = layer.layer(tower::service_fn(ok_handler));

        let req = Request::builder().body(Body::empty()).unwrap();
        let resp = svc.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn require_authenticated_does_not_call_inner_on_reject() {
        use std::sync::atomic::{AtomicBool, Ordering};

        let called = Arc::new(AtomicBool::new(false));
        let called_clone = called.clone();

        let layer = require_authenticated();
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
}
