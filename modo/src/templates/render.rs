use super::context::TemplateContext;
use super::engine::TemplateEngine;
use super::view::View;
use axum::body::Body;
use axum::http::Request;
use axum::response::{Html, IntoResponse, Response};
use futures_util::future::BoxFuture;
use http::StatusCode;
use std::sync::Arc;
use std::task::{Context, Poll};
use tower::{Layer, Service};
use tracing::{error, warn};

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
        let template_ctx = match request.extensions().get::<TemplateContext>().cloned() {
            Some(ctx) => ctx,
            None => {
                warn!(
                    "TemplateContext not found in request extensions; was TemplateContextLayer applied?"
                );
                TemplateContext::default()
            }
        };
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
            let merged = template_ctx.merge_with(view.user_context);

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
                    let error =
                        crate::error::Error::internal(format!("template render failed: {err}"));
                    Ok(error.into_response())
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::Error;
    use axum::Router;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::response::IntoResponse;
    use axum::routing::get;
    use std::sync::Arc;
    use tower::ServiceExt;

    #[tokio::test]
    async fn render_error_inserts_error_into_extensions() {
        // Create engine with no templates
        let engine = Arc::new(
            crate::templates::engine(&crate::templates::TemplateConfig {
                path: "/nonexistent_path_for_test".to_string(),
                ..Default::default()
            })
            .unwrap(),
        );

        // Handler that returns a View pointing to a nonexistent template
        let app = Router::new()
            .route(
                "/",
                get(|| async {
                    let view = crate::templates::View::new(
                        "nonexistent.html",
                        minijinja::Value::UNDEFINED,
                    );
                    let mut resp = StatusCode::OK.into_response();
                    resp.extensions_mut().insert(view);
                    resp
                }),
            )
            .layer(RenderLayer::new(engine))
            .layer(crate::templates::TemplateContextLayer::new());

        let resp = app
            .oneshot(Request::get("/").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
        // The Error should be in extensions for error_handler_middleware to pick up
        assert!(
            resp.extensions().get::<Error>().is_some(),
            "Expected Error in response extensions for error handler interception"
        );
    }
}
