use crate::context::TemplateContext;
use crate::engine::TemplateEngine;
use crate::view::View;
use axum::body::Body;
use axum::http::Request;
use axum::response::{Html, IntoResponse, Response};
use futures_util::future::BoxFuture;
use http::StatusCode;
use std::sync::Arc;
use std::task::{Context, Poll};
use tower::{Layer, Service};
use tracing::error;

/// Layer that intercepts View responses and renders them via the TemplateEngine.
/// Merges request TemplateContext with the view's user context.
#[derive(Clone)]
pub struct RenderLayer {
    engine: Arc<TemplateEngine>,
}

impl RenderLayer {
    pub fn new(engine: Arc<TemplateEngine>) -> Self {
        Self { engine }
    }
}

impl<S> Layer<S> for RenderLayer {
    type Service = RenderMiddleware<S>;

    fn layer(&self, inner: S) -> Self::Service {
        RenderMiddleware {
            inner,
            engine: self.engine.clone(),
        }
    }
}

#[derive(Clone)]
pub struct RenderMiddleware<S> {
    inner: S,
    engine: Arc<TemplateEngine>,
}

impl<S> Service<Request<Body>> for RenderMiddleware<S>
where
    S: Service<Request<Body>, Response = Response> + Clone + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Into<std::convert::Infallible> + 'static,
{
    type Response = Response;
    type Error = S::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request<Body>) -> Self::Future {
        let engine = self.engine.clone();
        let mut inner = self.inner.clone();

        // Capture request context and HTMX header before passing to handler
        let template_ctx = request
            .extensions()
            .get::<TemplateContext>()
            .cloned()
            .unwrap_or_default();
        let is_htmx = request.headers().get("hx-request").is_some();

        Box::pin(async move {
            let mut response = inner.call(request).await?;

            // Only process responses that contain a View
            let Some(view) = response.extensions_mut().remove::<View>() else {
                return Ok(response);
            };

            let status = response.status();

            // HTMX rule: non-200 status -> don't render, pass through
            if is_htmx && status != StatusCode::OK {
                return Ok(response);
            }

            // Pick template: htmx template for HTMX requests, full template otherwise
            let template_name = if is_htmx {
                view.htmx_template.as_deref().unwrap_or(&view.template)
            } else {
                &view.template
            };

            // Merge request context with user context
            let merged = merge_contexts(template_ctx, view.user_context);

            match engine.render(template_name, merged) {
                Ok(html) => {
                    let mut resp = Html(html).into_response();
                    // HTMX responses are always 200
                    if is_htmx {
                        *resp.status_mut() = StatusCode::OK;
                    } else {
                        *resp.status_mut() = status;
                    }
                    Ok(resp)
                }
                Err(err) => {
                    error!(template = template_name, error = %err, "template render failed");
                    Ok(StatusCode::INTERNAL_SERVER_ERROR.into_response())
                }
            }
        })
    }
}

/// Merge request-scoped context (locale, csrf, etc.) with user context (struct fields).
/// User context values take precedence over request context on key collision.
fn merge_contexts(request_ctx: TemplateContext, user_ctx: minijinja::Value) -> minijinja::Value {
    let mut map = request_ctx.into_values();

    // user_ctx is a struct serialized to Value — iterate its keys
    if let Ok(keys) = user_ctx.try_iter() {
        for key in keys {
            let k_str: String = key.to_string();
            if let Ok(val) = user_ctx.get_attr(&k_str) {
                map.insert(k_str, val);
            }
        }
    }

    minijinja::Value::from_serialize(&map)
}
