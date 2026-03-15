use super::context::TemplateContext;
use axum::http::Request;
use futures_util::future::BoxFuture;
use std::task::{Context, Poll};
use tower::{Layer, Service};

/// Layer that creates a `TemplateContext` in request extensions
/// with built-in values (current_url).
/// Must be applied outermost of all context-writing middleware.
#[derive(Clone, Default)]
pub struct ContextLayer;

impl ContextLayer {
    pub fn new() -> Self {
        Self
    }
}

impl<S> Layer<S> for ContextLayer {
    type Service = ContextMiddleware<S>;

    fn layer(&self, inner: S) -> Self::Service {
        ContextMiddleware { inner }
    }
}

#[derive(Clone)]
pub struct ContextMiddleware<S> {
    inner: S,
}

impl<S, ReqBody, ResBody> Service<Request<ReqBody>> for ContextMiddleware<S>
where
    S: Service<Request<ReqBody>, Response = axum::http::Response<ResBody>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    ReqBody: Send + 'static,
    ResBody: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request<ReqBody>) -> Self::Future {
        let mut inner = self.inner.clone();

        Box::pin(async move {
            let (mut parts, body) = request.into_parts();

            let mut ctx = parts
                .extensions
                .remove::<TemplateContext>()
                .unwrap_or_default();
            ctx.insert("current_url", parts.uri.to_string());
            parts.extensions.insert(ctx);

            let request = Request::from_parts(parts, body);
            inner.call(request).await
        })
    }
}
