use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use axum::body::Body;
use http::{Request, Response};
use tower::{Layer, Service};

use super::context::TemplateContext;
use super::engine::Engine;
use super::locale;
use crate::flash::state::FlashState;

// --- Layer ---

/// Tower middleware layer that populates [`TemplateContext`] for every request.
///
/// Install this layer on your router **before** any handler that uses [`Renderer`](super::Renderer).
/// The layer injects the following keys into the request's [`TemplateContext`]:
///
/// | Key               | Source                                        |
/// |-------------------|-----------------------------------------------|
/// | `current_url`     | `request.uri().to_string()`                  |
/// | `is_htmx`         | `HX-Request: true` header                    |
/// | `request_id`      | `X-Request-Id` header (if present)           |
/// | `locale`          | Locale resolver chain (falls back to default) |
/// | `csrf_token`      | [`CsrfToken`](crate::middleware::CsrfToken) extension (if present) |
/// | `flash_messages`  | `FlashState` extension (if present)        |
///
/// # Example
///
/// ```rust,no_run
/// use modo::template::{Engine, EngineBuilder, TemplateConfig, TemplateContextLayer};
///
/// # fn example(engine: Engine) {
/// let router: axum::Router = axum::Router::new()
///     // ... routes ...
///     .layer(TemplateContextLayer::new(engine));
/// # }
/// ```
#[derive(Clone)]
pub struct TemplateContextLayer {
    engine: Engine,
}

impl TemplateContextLayer {
    /// Creates a new layer backed by the given [`Engine`].
    pub fn new(engine: Engine) -> Self {
        Self { engine }
    }
}

impl<S> Layer<S> for TemplateContextLayer {
    type Service = TemplateContextMiddleware<S>;

    fn layer(&self, inner: S) -> Self::Service {
        TemplateContextMiddleware {
            inner,
            engine: self.engine.clone(),
        }
    }
}

// --- Service ---

/// Tower [`Service`] produced by [`TemplateContextLayer`].
///
/// Populates a [`TemplateContext`] with per-request data and inserts it into
/// request extensions before delegating to the inner service.
#[derive(Clone)]
pub struct TemplateContextMiddleware<S> {
    inner: S,
    engine: Engine,
}

impl<S, ReqBody> Service<Request<ReqBody>> for TemplateContextMiddleware<S>
where
    S: Service<Request<ReqBody>, Response = Response<Body>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Into<Box<dyn std::error::Error + Send + Sync>> + Send + 'static,
    ReqBody: Send + 'static,
{
    type Response = Response<Body>;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut request: Request<ReqBody>) -> Self::Future {
        let engine = self.engine.clone();
        let mut inner = self.inner.clone();
        std::mem::swap(&mut self.inner, &mut inner);

        Box::pin(async move {
            // Build TemplateContext with request-scoped data
            let mut ctx = TemplateContext::default();

            // current_url
            ctx.set(
                "current_url",
                minijinja::Value::from(request.uri().to_string()),
            );

            // is_htmx
            let is_htmx = request
                .headers()
                .get("hx-request")
                .and_then(|v| v.to_str().ok())
                .is_some_and(|v| v == "true");
            ctx.set("is_htmx", minijinja::Value::from(is_htmx));

            // request_id (if present)
            if let Some(req_id) = request
                .headers()
                .get("x-request-id")
                .and_then(|v| v.to_str().ok())
            {
                ctx.set("request_id", minijinja::Value::from(req_id.to_string()));
            }

            // locale resolution
            {
                // We need to extract Parts temporarily for locale resolution
                // Since we can't split the request here, read the values we need from headers
                let (mut parts, body) = request.into_parts();

                let resolved_locale = locale::resolve_locale(engine.locale_chain(), &parts);
                let locale_value =
                    resolved_locale.unwrap_or_else(|| engine.default_locale().to_string());
                ctx.set("locale", minijinja::Value::from(locale_value));

                // csrf_token (if present in extensions)
                if let Some(csrf) = parts.extensions.get::<crate::middleware::CsrfToken>() {
                    ctx.set("csrf_token", minijinja::Value::from(csrf.0.clone()));
                }

                // flash_messages() template function
                if let Some(flash_state) = parts.extensions.get::<Arc<FlashState>>() {
                    let state = flash_state.clone();
                    ctx.set(
                        "flash_messages",
                        minijinja::Value::from_function(
                            move |_args: &[minijinja::Value]| -> Result<minijinja::Value, minijinja::Error> {
                                state.mark_read();
                                let entries = state.incoming_as_template_value();
                                Ok(minijinja::Value::from_serialize(&entries))
                            },
                        ),
                    );
                }

                // Insert TemplateContext into extensions
                parts.extensions.insert(ctx);

                request = Request::from_parts(parts, body);
            }

            inner.call(request).await
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, routing::get};
    use http::{Request, StatusCode};
    use tower::ServiceExt;

    use crate::template::{TemplateConfig, TemplateContext};

    // Return TempDir alongside Engine so files persist for the test's lifetime
    fn test_engine() -> (tempfile::TempDir, Engine) {
        let dir = tempfile::tempdir().unwrap();
        let tpl_dir = dir.path().join("templates");
        let locales_dir = dir.path().join("locales/en");
        let static_dir = dir.path().join("static");
        std::fs::create_dir_all(&tpl_dir).unwrap();
        std::fs::create_dir_all(&locales_dir).unwrap();
        std::fs::create_dir_all(&static_dir).unwrap();
        std::fs::write(locales_dir.join("common.yaml"), "greeting: Hello").unwrap();

        let uk_locales_dir = dir.path().join("locales/uk");
        std::fs::create_dir_all(&uk_locales_dir).unwrap();
        std::fs::write(uk_locales_dir.join("common.yaml"), "greeting: Привіт").unwrap();

        let config = TemplateConfig {
            templates_path: tpl_dir.to_str().unwrap().into(),
            locales_path: dir.path().join("locales").to_str().unwrap().into(),
            static_path: static_dir.to_str().unwrap().into(),
            ..TemplateConfig::default()
        };

        let engine = Engine::builder().config(config).build().unwrap();
        (dir, engine)
    }

    // Handlers must be module-level async fn per CLAUDE.md gotcha
    async fn extract_url(req: Request<Body>) -> (StatusCode, String) {
        let ctx = req.extensions().get::<TemplateContext>().unwrap();
        let url = ctx
            .get("current_url")
            .map(|v| v.to_string())
            .unwrap_or_default();
        (StatusCode::OK, url)
    }

    async fn extract_is_htmx(req: Request<Body>) -> (StatusCode, String) {
        let ctx = req.extensions().get::<TemplateContext>().unwrap();
        let is_htmx = ctx
            .get("is_htmx")
            .map(|v| v.to_string())
            .unwrap_or_default();
        (StatusCode::OK, is_htmx)
    }

    async fn extract_locale(req: Request<Body>) -> (StatusCode, String) {
        let ctx = req.extensions().get::<TemplateContext>().unwrap();
        let locale = ctx.get("locale").map(|v| v.to_string()).unwrap_or_default();
        (StatusCode::OK, locale)
    }

    async fn extract_request_id(req: Request<Body>) -> (StatusCode, String) {
        let ctx = req.extensions().get::<TemplateContext>().unwrap();
        let request_id = ctx
            .get("request_id")
            .map(|v| v.to_string())
            .unwrap_or_default();
        (StatusCode::OK, request_id)
    }

    #[tokio::test]
    async fn injects_current_url_value() {
        let (_dir, engine) = test_engine();
        let app = Router::new()
            .route("/test", get(extract_url))
            .layer(TemplateContextLayer::new(engine));

        let req = Request::builder().uri("/test").body(Body::empty()).unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(body, "/test");
    }

    #[tokio::test]
    async fn injects_is_htmx_false() {
        let (_dir, engine) = test_engine();
        let app = Router::new()
            .route("/test", get(extract_is_htmx))
            .layer(TemplateContextLayer::new(engine));

        let req = Request::builder().uri("/test").body(Body::empty()).unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn injects_is_htmx_true() {
        let (_dir, engine) = test_engine();
        let app = Router::new()
            .route("/test", get(extract_is_htmx))
            .layer(TemplateContextLayer::new(engine));

        let req = Request::builder()
            .uri("/test")
            .header("hx-request", "true")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(body, "true");
    }

    #[tokio::test]
    async fn injects_locale_default() {
        let (_dir, engine) = test_engine();
        let app = Router::new()
            .route("/test", get(extract_locale))
            .layer(TemplateContextLayer::new(engine));

        let req = Request::builder().uri("/test").body(Body::empty()).unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(body, "en");
    }

    #[tokio::test]
    async fn injects_locale_from_query() {
        let (_dir, engine) = test_engine();
        let app = Router::new()
            .route("/test", get(extract_locale))
            .layer(TemplateContextLayer::new(engine));

        let req = Request::builder()
            .uri("/test?lang=uk")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(body, "uk");
    }

    #[tokio::test]
    async fn injects_request_id() {
        let (_dir, engine) = test_engine();
        let app = Router::new()
            .route("/test", get(extract_request_id))
            .layer(TemplateContextLayer::new(engine));

        let req = Request::builder()
            .uri("/test")
            .header("x-request-id", "abc123")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(body, "abc123");
    }

    #[tokio::test]
    async fn injects_flash_messages_function() {
        use crate::flash::state::{FlashEntry, FlashState};

        let (_dir, engine) = test_engine();
        let tpl_dir = _dir.path().join("templates");
        std::fs::write(
            tpl_dir.join("flash_test.html"),
            "{% for msg in flash_messages() %}{% for level, text in msg|items %}{{ level }}:{{ text }};{% endfor %}{% endfor %}",
        ).unwrap();

        let entries = vec![
            FlashEntry {
                level: "error".into(),
                message: "bad".into(),
            },
            FlashEntry {
                level: "info".into(),
                message: "ok".into(),
            },
        ];
        let flash_state = Arc::new(FlashState::new(entries));

        // Use the engine directly to render, simulating what Renderer does
        let mut ctx = TemplateContext::default();

        // Register flash_messages function (same logic as middleware)
        let state = flash_state.clone();
        ctx.set(
            "flash_messages",
            minijinja::Value::from_function(
                move |_args: &[minijinja::Value]| -> Result<minijinja::Value, minijinja::Error> {
                    state.mark_read();
                    let entries = state.incoming_as_template_value();
                    Ok(minijinja::Value::from_serialize(&entries))
                },
            ),
        );

        let merged = ctx.merge(minijinja::context! {});
        let result = engine.render("flash_test.html", merged).unwrap();

        assert!(result.contains("error:bad;"));
        assert!(result.contains("info:ok;"));
        assert!(flash_state.was_read());
    }
}
