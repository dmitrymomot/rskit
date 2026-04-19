//! Route-level gating layers — `require_authenticated`, `require_role`,
//! `require_scope`.
//!
//! Provides guard layers that reject requests based on authentication state,
//! role membership, or API key scope. All guards run after route matching
//! (`.route_layer()`) and expect upstream middleware (role extractor,
//! [`ApiKeyLayer`](crate::auth::apikey::ApiKeyLayer)) to have populated
//! extensions before the guard executes.

use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use axum::body::Body;
use axum::response::IntoResponse;
use http::Request;
use tower::{Layer, Service};

use crate::Error;
use crate::auth::apikey::ApiKeyMeta;
use crate::auth::role::Role;

// --- shared redirect helper ---

/// Build a redirect response for guard short-circuits.
///
/// For htmx requests (`hx-request: true`), returns `200 OK` with the
/// `HX-Redirect: <path>` header so htmx performs the client-side navigation.
/// For all other requests, returns `303 See Other` with `Location: <path>`.
#[allow(dead_code)] // Used by Tasks 2/3 guards (require_authenticated rewrite, require_guest_only).
fn redirect_response(path: &str, headers: &http::HeaderMap) -> http::Response<Body> {
    let is_htmx = headers.get("hx-request").and_then(|v| v.to_str().ok()) == Some("true");

    let mut response = http::Response::new(Body::empty());
    if is_htmx {
        *response.status_mut() = http::StatusCode::OK;
        if let Ok(value) = http::HeaderValue::from_str(path) {
            response.headers_mut().insert("hx-redirect", value);
        }
    } else {
        *response.status_mut() = http::StatusCode::SEE_OTHER;
        if let Ok(value) = http::HeaderValue::from_str(path) {
            response.headers_mut().insert(http::header::LOCATION, value);
        }
    }
    response
}

// --- require_role ---

/// Creates a guard layer that rejects requests unless the resolved
/// [`Role`] matches ANY of the allowed roles. Exact string match only;
/// there is no hierarchy.
///
/// # Status codes
///
/// - **401 Unauthorized** — no [`Role`] in request extensions (upstream
///   middleware never populated one).
/// - **403 Forbidden** — a role is present but not in the allowed list.
///   An empty `roles` iterator always returns 403.
///
/// # Wiring
///
/// Apply with `.route_layer()` so the guard runs after route matching.
/// A role-resolving middleware (e.g. from [`crate::auth::role`]) must run
/// earlier via `.layer()` so that [`Role`] is in extensions when this
/// guard runs.
///
/// # Example
///
/// ```rust,no_run
/// # fn example() {
/// use axum::Router;
/// use axum::routing::get;
/// use modo::auth::guard::require_role;
///
/// let app: Router = Router::new()
///     .route("/admin", get(|| async { "admin area" }))
///     .route_layer(require_role(["admin", "owner"]));
/// # }
/// ```
pub fn require_role(roles: impl IntoIterator<Item = impl Into<String>>) -> RequireRoleLayer {
    RequireRoleLayer {
        roles: Arc::new(roles.into_iter().map(Into::into).collect()),
    }
}

/// Tower layer produced by [`require_role()`].
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

/// Tower service produced by [`RequireRoleLayer`].
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

/// Creates a guard layer that rejects requests unless a [`Role`] is
/// present in extensions. The role's value is not inspected — any
/// resolved role is accepted.
///
/// # Status codes
///
/// - **401 Unauthorized** — no [`Role`] in request extensions.
///
/// # Wiring
///
/// Apply with `.route_layer()` so the guard runs after route matching.
/// A role-resolving middleware (e.g. from [`crate::auth::role`]) must run
/// earlier via `.layer()` so that [`Role`] is in extensions when this
/// guard runs.
///
/// # Example
///
/// ```rust,no_run
/// # fn example() {
/// use axum::Router;
/// use axum::routing::get;
/// use modo::auth::guard::require_authenticated;
///
/// let app: Router = Router::new()
///     .route("/me", get(|| async { "profile" }))
///     .route_layer(require_authenticated());
/// # }
/// ```
pub fn require_authenticated() -> RequireAuthenticatedLayer {
    RequireAuthenticatedLayer
}

/// Tower layer produced by [`require_authenticated()`].
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

/// Tower service produced by [`RequireAuthenticatedLayer`].
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

// --- require_scope ---

/// Creates a guard layer that rejects requests unless the verified API
/// key's scope list contains the required scope. Uses exact string
/// matching; there is no wildcard or hierarchy.
///
/// # Status codes
///
/// - **500 Internal Server Error** — no [`ApiKeyMeta`] in request
///   extensions. The guard is fail-closed and logs an error; this state
///   indicates the wiring is wrong (missing
///   [`ApiKeyLayer`](crate::auth::apikey::ApiKeyLayer) upstream).
/// - **403 Forbidden** — the API key is present but does not carry the
///   required scope.
///
/// # Wiring
///
/// Apply with `.route_layer()` so the guard runs after route matching.
/// [`ApiKeyLayer`](crate::auth::apikey::ApiKeyLayer) must run earlier
/// (via `.layer()`) so that [`ApiKeyMeta`] is in extensions when this
/// guard runs.
///
/// # Example
///
/// ```rust,no_run
/// # fn example() {
/// use axum::Router;
/// use axum::routing::get;
/// use modo::auth::guard::require_scope;
///
/// let app: Router = Router::new()
///     .route("/orders", get(|| async { "orders" }))
///     .route_layer(require_scope("read:orders"));
/// # }
/// ```
pub fn require_scope(scope: &str) -> ScopeLayer {
    ScopeLayer {
        scope: scope.to_owned(),
    }
}

/// Tower [`Layer`] that checks for a required scope on the verified API key.
///
/// Created by [`require_scope`]. Apply as a `.route_layer()` after
/// [`ApiKeyLayer`](crate::auth::apikey::ApiKeyLayer).
#[derive(Clone)]
pub struct ScopeLayer {
    scope: String,
}

impl<S> Layer<S> for ScopeLayer {
    type Service = ScopeMiddleware<S>;

    fn layer(&self, inner: S) -> Self::Service {
        ScopeMiddleware {
            inner,
            scope: self.scope.clone(),
        }
    }
}

/// Tower [`Service`] that checks for a required scope.
pub struct ScopeMiddleware<S> {
    inner: S,
    scope: String,
}

impl<S: Clone> Clone for ScopeMiddleware<S> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            scope: self.scope.clone(),
        }
    }
}

impl<S> Service<Request<Body>> for ScopeMiddleware<S>
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
        let scope = self.scope.clone();
        let mut inner = self.inner.clone();
        std::mem::swap(&mut self.inner, &mut inner);

        Box::pin(async move {
            let Some(meta) = request.extensions().get::<ApiKeyMeta>() else {
                tracing::error!(
                    "require_scope guard reached without an API key in extensions; \
                     ApiKeyLayer must run before this guard"
                );
                return Ok(Error::internal("server misconfigured").into_response());
            };

            if !meta.scopes.iter().any(|s| s == &scope) {
                return Ok(
                    Error::forbidden(format!("missing required scope: {scope}")).into_response()
                );
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

    // --- redirect_response helper tests ---

    #[test]
    fn redirect_response_non_htmx_returns_303_with_location() {
        let headers = http::HeaderMap::new();
        let resp = redirect_response("/auth", &headers);
        assert_eq!(resp.status(), StatusCode::SEE_OTHER);
        assert_eq!(resp.headers().get(http::header::LOCATION).unwrap(), "/auth");
        assert!(resp.headers().get("hx-redirect").is_none());
    }

    #[test]
    fn redirect_response_htmx_returns_200_with_hx_redirect() {
        let mut headers = http::HeaderMap::new();
        headers.insert("hx-request", http::HeaderValue::from_static("true"));
        let resp = redirect_response("/app", &headers);
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(resp.headers().get("hx-redirect").unwrap(), "/app");
        assert!(resp.headers().get(http::header::LOCATION).is_none());
    }

    #[test]
    fn redirect_response_hx_request_false_uses_303() {
        let mut headers = http::HeaderMap::new();
        headers.insert("hx-request", http::HeaderValue::from_static("false"));
        let resp = redirect_response("/x", &headers);
        assert_eq!(resp.status(), StatusCode::SEE_OTHER);
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

    // --- require_scope tests ---

    fn meta_with_scopes(scopes: &[&str]) -> ApiKeyMeta {
        ApiKeyMeta {
            id: "01HX".into(),
            tenant_id: "t".into(),
            name: "test key".into(),
            scopes: scopes.iter().map(|s| (*s).into()).collect(),
            expires_at: None,
            last_used_at: None,
            created_at: "2026-01-01T00:00:00Z".into(),
        }
    }

    #[tokio::test]
    async fn require_scope_passes_when_scope_present() {
        let layer = require_scope("read:orders");
        let svc = layer.layer(tower::service_fn(ok_handler));

        let mut req = Request::builder().body(Body::empty()).unwrap();
        req.extensions_mut()
            .insert(meta_with_scopes(&["read:orders", "write:orders"]));
        let resp = svc.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn require_scope_403_when_scope_absent() {
        let layer = require_scope("admin:all");
        let svc = layer.layer(tower::service_fn(ok_handler));

        let mut req = Request::builder().body(Body::empty()).unwrap();
        req.extensions_mut()
            .insert(meta_with_scopes(&["read:orders"]));
        let resp = svc.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn require_scope_500_when_apikey_meta_missing() {
        let layer = require_scope("read:orders");
        let svc = layer.layer(tower::service_fn(ok_handler));

        let req = Request::builder().body(Body::empty()).unwrap();
        let resp = svc.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }
}
