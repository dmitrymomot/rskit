use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use axum::body::Body;
use axum::response::IntoResponse;
use http::Request;
use tower::{Layer, Service};

use crate::error::Error;

use super::types::ApiKeyMeta;

/// Create a route layer that requires the verified API key to have a
/// specific scope.
///
/// Uses exact string matching. Must be applied after [`super::ApiKeyLayer`].
///
/// # Example
///
/// ```rust,no_run
/// # #[cfg(feature = "apikey")]
/// # fn example() {
/// use axum::Router;
/// use axum::routing::get;
/// use modo::apikey::require_scope;
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
                return Ok(
                    Error::internal("require_scope() called without ApiKeyLayer").into_response(),
                );
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
