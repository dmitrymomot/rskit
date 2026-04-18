use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use axum::body::Body;
use http::{Request, Response};
use tower::{Layer, Service};

use super::context::TemplateContext;
use crate::flash::state::FlashState;
use crate::i18n::Translator;

// --- Layer ---

/// Tower middleware layer that populates [`TemplateContext`] for every request.
///
/// Install this layer on your router **before** any handler that uses
/// [`Renderer`](super::Renderer). The layer injects the following keys into
/// the request's [`TemplateContext`]:
///
/// | Key               | Source                                                              |
/// |-------------------|---------------------------------------------------------------------|
/// | `current_url`     | `request.uri().to_string()`                                         |
/// | `is_htmx`         | `HX-Request: true` header                                           |
/// | `request_id`      | `X-Request-Id` header (if present)                                  |
/// | `locale`          | [`Translator`](crate::i18n::Translator) in extensions (present only when [`I18nLayer`](crate::i18n::I18nLayer) is installed upstream) |
/// | `csrf_token`      | [`CsrfToken`](crate::middleware::CsrfToken) extension (if present)  |
/// | `flash_messages`  | Callable returning flash entries; `FlashState` extension must be set by [`FlashLayer`](crate::flash::FlashLayer) |
/// | `tier_name`       | `TierInfo::name` (when `TierInfo` extension is present)             |
/// | `tier_has`        | Template function `tier_has(name) -> bool` (when `TierInfo` is present) |
/// | `tier_enabled`    | Template function `tier_enabled(name) -> bool` (when `TierInfo` is present) |
/// | `tier_limit`      | Template function `tier_limit(name) -> Option<u64>` (when `TierInfo` is present) |
///
/// This layer reads the current request's
/// [`Translator`](crate::i18n::Translator) (installed by
/// [`I18nLayer`](crate::i18n::I18nLayer)) and exposes `locale` to templates.
/// If no `I18nLayer` is upstream, the `locale` variable is simply absent from
/// the template context.
///
/// This layer is also re-exported as
/// [`modo::middlewares::TemplateContext`](crate::middlewares::TemplateContext)
/// for convenience at wiring sites.
///
/// # Example
///
/// ```rust,no_run
/// use modo::template::TemplateContextLayer;
///
/// let router: axum::Router = axum::Router::new()
///     // ... routes ...
///     .layer(TemplateContextLayer::new());
/// ```
#[derive(Clone, Default)]
pub struct TemplateContextLayer;

impl TemplateContextLayer {
    /// Creates a new [`TemplateContextLayer`].
    pub fn new() -> Self {
        Self
    }
}

impl<S> Layer<S> for TemplateContextLayer {
    type Service = TemplateContextMiddleware<S>;

    fn layer(&self, inner: S) -> Self::Service {
        TemplateContextMiddleware { inner }
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

            // Read request extensions for locale, csrf, flash, tier.
            {
                let (mut parts, body) = request.into_parts();

                // locale comes from the Translator installed upstream by I18nLayer.
                // If no I18nLayer is installed, `locale` is simply absent from
                // the template context — the template engine's MiniJinja state
                // then sees `locale` as undefined, which is the correct signal
                // that `t()` cannot be used.
                if let Some(translator) = parts.extensions.get::<Translator>() {
                    ctx.set(
                        "locale",
                        minijinja::Value::from(translator.locale().to_string()),
                    );
                }

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

                // tier info (if tier feature enabled and TierInfo in extensions)
                if let Some(tier_info) = parts.extensions.get::<crate::tier::TierInfo>() {
                    ctx.set("tier_name", minijinja::Value::from(tier_info.name.clone()));

                    let ti = Arc::new(tier_info.clone());

                    let t = ti.clone();
                    ctx.set(
                        "tier_has",
                        minijinja::Value::from_function(move |name: &str| -> bool {
                            t.has_feature(name)
                        }),
                    );

                    let t = ti.clone();
                    ctx.set(
                        "tier_enabled",
                        minijinja::Value::from_function(move |name: &str| -> bool {
                            t.is_enabled(name)
                        }),
                    );

                    ctx.set(
                        "tier_limit",
                        minijinja::Value::from_function(move |name: &str| -> Option<u64> {
                            ti.limit(name)
                        }),
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
    use axum::{Router, body::Body, routing::get};
    use http::{Request, StatusCode};
    use tower::ServiceExt;

    use crate::i18n::{I18n, I18nConfig};
    use crate::template::{Engine, TemplateConfig, TemplateContext};

    // Return TempDir alongside Engine so files persist for the test's lifetime
    fn test_engine() -> (tempfile::TempDir, Engine) {
        let dir = tempfile::tempdir().unwrap();
        let tpl_dir = dir.path().join("templates");
        let static_dir = dir.path().join("static");
        std::fs::create_dir_all(&tpl_dir).unwrap();
        std::fs::create_dir_all(&static_dir).unwrap();

        let config = TemplateConfig {
            templates_path: tpl_dir.to_str().unwrap().into(),
            static_path: static_dir.to_str().unwrap().into(),
            ..TemplateConfig::default()
        };

        let engine = Engine::builder().config(config).build().unwrap();
        (dir, engine)
    }

    fn test_i18n(dir: &std::path::Path) -> I18n {
        let locales_dir = dir.join("locales");
        std::fs::create_dir_all(locales_dir.join("en")).unwrap();
        std::fs::write(
            locales_dir.join("en").join("common.yaml"),
            "greeting: Hello",
        )
        .unwrap();
        std::fs::create_dir_all(locales_dir.join("uk")).unwrap();
        std::fs::write(
            locales_dir.join("uk").join("common.yaml"),
            "greeting: Привіт",
        )
        .unwrap();

        let config = I18nConfig {
            locales_path: locales_dir.to_str().unwrap().into(),
            default_locale: "en".into(),
            ..I18nConfig::default()
        };
        I18n::new(&config).unwrap()
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
        let (_dir, _engine) = test_engine();
        let app = Router::new()
            .route("/test", get(extract_url))
            .layer(TemplateContextLayer::new());

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
        let (_dir, _engine) = test_engine();
        let app = Router::new()
            .route("/test", get(extract_is_htmx))
            .layer(TemplateContextLayer::new());

        let req = Request::builder().uri("/test").body(Body::empty()).unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn injects_is_htmx_true() {
        let (_dir, _engine) = test_engine();
        let app = Router::new()
            .route("/test", get(extract_is_htmx))
            .layer(TemplateContextLayer::new());

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
        let (_dir, _engine) = test_engine();
        let i18n = test_i18n(_dir.path());
        // I18nLayer must run before TemplateContextLayer sees the request, so
        // install I18nLayer as an outer layer (outer layers run first in axum).
        let app = Router::new()
            .route("/test", get(extract_locale))
            .layer(TemplateContextLayer::new())
            .layer(i18n.layer());

        let req = Request::builder().uri("/test").body(Body::empty()).unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(body, "en");
    }

    #[tokio::test]
    async fn injects_locale_from_query() {
        let (_dir, _engine) = test_engine();
        let i18n = test_i18n(_dir.path());
        let app = Router::new()
            .route("/test", get(extract_locale))
            .layer(TemplateContextLayer::new())
            .layer(i18n.layer());

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
    async fn no_i18n_layer_omits_locale() {
        let (_dir, _engine) = test_engine();
        // No I18nLayer installed.
        let app = Router::new()
            .route("/test", get(extract_locale))
            .layer(TemplateContextLayer::new());

        let req = Request::builder().uri("/test").body(Body::empty()).unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        // locale not set — returns empty string
        assert_eq!(body, "");
    }

    #[tokio::test]
    async fn injects_request_id() {
        let (_dir, _engine) = test_engine();
        let app = Router::new()
            .route("/test", get(extract_request_id))
            .layer(TemplateContextLayer::new());

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

    mod tier_tests {
        use super::*;
        use std::collections::HashMap;

        use crate::tier::{FeatureAccess, TierInfo};

        fn test_tier() -> TierInfo {
            TierInfo {
                name: "pro".into(),
                features: HashMap::from([
                    ("sso".into(), FeatureAccess::Toggle(true)),
                    ("custom_domain".into(), FeatureAccess::Toggle(false)),
                    ("api_calls".into(), FeatureAccess::Limit(100_000)),
                ]),
            }
        }

        async fn extract_tier_name(req: Request<Body>) -> (StatusCode, String) {
            let ctx = req.extensions().get::<TemplateContext>().unwrap();
            let name = ctx
                .get("tier_name")
                .map(|v| v.to_string())
                .unwrap_or_default();
            (StatusCode::OK, name)
        }

        #[tokio::test]
        async fn injects_tier_name() {
            let (_dir, _engine) = test_engine();
            let app = Router::new()
                .route("/test", get(extract_tier_name))
                .layer(TemplateContextLayer::new());

            let mut req = Request::builder().uri("/test").body(Body::empty()).unwrap();
            req.extensions_mut().insert(test_tier());
            let resp = app.oneshot(req).await.unwrap();
            let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
                .await
                .unwrap();
            assert_eq!(body, "pro");
        }

        #[tokio::test]
        async fn tier_has_function_works() {
            let (_dir, engine) = test_engine();
            let tpl_dir = _dir.path().join("templates");
            std::fs::write(
                tpl_dir.join("tier_has_test.html"),
                "{% if tier_has('sso') %}yes{% else %}no{% endif %}",
            )
            .unwrap();

            let mut ctx = TemplateContext::default();
            let tier = test_tier();
            ctx.set("tier_name", minijinja::Value::from(tier.name.clone()));

            let ti = tier.clone();
            ctx.set(
                "tier_has",
                minijinja::Value::from_function(move |name: &str| -> bool { ti.has_feature(name) }),
            );

            let merged = ctx.merge(minijinja::context! {});
            let result = engine.render("tier_has_test.html", merged).unwrap();
            assert_eq!(result, "yes");
        }

        #[tokio::test]
        async fn tier_has_returns_false_for_disabled() {
            let (_dir, engine) = test_engine();
            let tpl_dir = _dir.path().join("templates");
            std::fs::write(
                tpl_dir.join("tier_disabled_test.html"),
                "{% if tier_has('custom_domain') %}yes{% else %}no{% endif %}",
            )
            .unwrap();

            let mut ctx = TemplateContext::default();
            let tier = test_tier();

            let ti = tier.clone();
            ctx.set(
                "tier_has",
                minijinja::Value::from_function(move |name: &str| -> bool { ti.has_feature(name) }),
            );

            let merged = ctx.merge(minijinja::context! {});
            let result = engine.render("tier_disabled_test.html", merged).unwrap();
            assert_eq!(result, "no");
        }

        #[tokio::test]
        async fn tier_limit_function_works() {
            let (_dir, engine) = test_engine();
            let tpl_dir = _dir.path().join("templates");
            std::fs::write(
                tpl_dir.join("tier_limit_test.html"),
                "{{ tier_limit('api_calls') }}",
            )
            .unwrap();

            let mut ctx = TemplateContext::default();
            let tier = test_tier();

            let ti = tier.clone();
            ctx.set(
                "tier_limit",
                minijinja::Value::from_function(move |name: &str| -> Option<u64> {
                    ti.limit(name)
                }),
            );

            let merged = ctx.merge(minijinja::context! {});
            let result = engine.render("tier_limit_test.html", merged).unwrap();
            assert_eq!(result, "100000");
        }

        #[tokio::test]
        async fn no_tier_info_no_injection() {
            let (_dir, _engine) = test_engine();
            let app = Router::new()
                .route("/test", get(extract_tier_name))
                .layer(TemplateContextLayer::new());

            let req = Request::builder().uri("/test").body(Body::empty()).unwrap();
            let resp = app.oneshot(req).await.unwrap();
            let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
                .await
                .unwrap();
            // tier_name not set — returns empty string
            assert_eq!(body, "");
        }
    }
}
