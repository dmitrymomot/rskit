use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use axum::body::Body;
use http::{Request, Response};
use tower::{Layer, Service};

use super::extractor::Translator;
use super::locale::{self, LocaleResolver};
use super::store::TranslationStore;

// --- Layer ---

/// Tower middleware layer that injects a [`Translator`] into every request.
///
/// Obtain via [`I18n::layer`](super::I18n::layer). The layer runs the locale
/// resolver chain against the request, falls back to the configured default
/// locale if nothing matches, and inserts the resulting [`Translator`] into
/// request extensions for handlers to extract.
#[derive(Clone)]
pub struct I18nLayer {
    chain: Vec<Arc<dyn LocaleResolver>>,
    store: TranslationStore,
    default_locale: String,
}

impl I18nLayer {
    pub(super) fn new(
        chain: Vec<Arc<dyn LocaleResolver>>,
        store: TranslationStore,
        default_locale: String,
    ) -> Self {
        Self {
            chain,
            store,
            default_locale,
        }
    }
}

impl<S> Layer<S> for I18nLayer {
    type Service = I18nMiddleware<S>;

    fn layer(&self, inner: S) -> Self::Service {
        I18nMiddleware {
            inner,
            chain: self.chain.clone(),
            store: self.store.clone(),
            default_locale: self.default_locale.clone(),
        }
    }
}

// --- Service ---

/// Tower [`Service`] produced by [`I18nLayer`].
pub struct I18nMiddleware<S> {
    inner: S,
    chain: Vec<Arc<dyn LocaleResolver>>,
    store: TranslationStore,
    default_locale: String,
}

impl<S: Clone> Clone for I18nMiddleware<S> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            chain: self.chain.clone(),
            store: self.store.clone(),
            default_locale: self.default_locale.clone(),
        }
    }
}

impl<S, ReqBody> Service<Request<ReqBody>> for I18nMiddleware<S>
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

    fn call(&mut self, request: Request<ReqBody>) -> Self::Future {
        let chain = self.chain.clone();
        let store = self.store.clone();
        let default_locale = self.default_locale.clone();
        let mut inner = self.inner.clone();
        std::mem::swap(&mut self.inner, &mut inner);

        Box::pin(async move {
            let (mut parts, body) = request.into_parts();

            let resolved = locale::resolve_locale(&chain, &parts).unwrap_or(default_locale);
            let translator = Translator::new(resolved, store);
            parts.extensions.insert(translator);

            let request = Request::from_parts(parts, body);
            inner.call(request).await
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::config::I18nConfig;
    use crate::i18n::factory::I18n;
    use axum::{Router, routing::get};
    use http::{Request, StatusCode};
    use tower::ServiceExt;

    fn test_i18n() -> (tempfile::TempDir, I18n) {
        let dir = tempfile::tempdir().unwrap();
        let en_dir = dir.path().join("en");
        let uk_dir = dir.path().join("uk");
        std::fs::create_dir_all(&en_dir).unwrap();
        std::fs::create_dir_all(&uk_dir).unwrap();
        std::fs::write(en_dir.join("common.yaml"), "greeting: Hello").unwrap();
        std::fs::write(uk_dir.join("common.yaml"), "greeting: Привіт").unwrap();

        let config = I18nConfig {
            locales_path: dir.path().to_str().unwrap().to_string(),
            default_locale: "en".into(),
            ..I18nConfig::default()
        };
        let i18n = I18n::new(&config).unwrap();
        (dir, i18n)
    }

    async fn read_locale(translator: Translator) -> (StatusCode, String) {
        (StatusCode::OK, translator.locale().to_string())
    }

    async fn read_greeting(translator: Translator) -> (StatusCode, String) {
        (StatusCode::OK, translator.t("common.greeting", &[]))
    }

    #[tokio::test]
    async fn resolves_default_locale() {
        let (_dir, i18n) = test_i18n();
        let app = Router::new()
            .route("/", get(read_locale))
            .layer(i18n.layer());

        let req = Request::builder().uri("/").body(Body::empty()).unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(body, "en");
    }

    #[tokio::test]
    async fn resolves_locale_from_query() {
        let (_dir, i18n) = test_i18n();
        let app = Router::new()
            .route("/", get(read_greeting))
            .layer(i18n.layer());

        let req = Request::builder()
            .uri("/?lang=uk")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(body, "Привіт");
    }
}
