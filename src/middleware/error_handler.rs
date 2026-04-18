use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use axum::response::{IntoResponse, Response};
use http::request::Parts;
use tower::{Layer, Service};

use crate::error::render_error_body;

/// Creates an error-handler layer that intercepts responses containing a
/// [`crate::error::Error`] in their extensions and rewrites them through
/// the supplied handler function.
///
/// Any middleware that stores a `modo::Error` in response extensions
/// (`Error::into_response()`, `catch_panic`, `csrf`, `rate_limit`, etc.)
/// will be caught by this layer, giving the application a single place to
/// control the error response format (JSON, HTML, plain text, etc.).
///
/// The handler receives the error and the original request parts (method,
/// URI, headers, extensions) by value.
///
/// # Example
///
/// ```rust,no_run
/// use axum::{Router, routing::get};
/// use axum::response::IntoResponse;
///
/// async fn render_error(err: modo::Error, parts: http::request::Parts) -> axum::response::Response {
///     (err.status(), err.message().to_string()).into_response()
/// }
///
/// let app: Router = Router::new()
///     .route("/", get(|| async { "ok" }))
///     .layer(modo::middleware::error_handler(render_error));
/// ```
pub fn error_handler<F, Fut>(handler: F) -> ErrorHandlerLayer<F>
where
    F: Fn(crate::error::Error, Parts) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = Response> + Send + 'static,
{
    ErrorHandlerLayer { handler }
}

/// Default error responder suitable for passing directly to [`error_handler`].
///
/// Produces the same JSON shape as [`crate::Error::into_response`]:
///
/// ```json
/// { "error": { "status": 404, "message": "..." } }
/// ```
///
/// When the error carries a translation key (via
/// [`Error::localized`](crate::Error::localized) or
/// [`Error::with_locale_key`](crate::Error::with_locale_key)) **and** the
/// request has a [`Translator`](crate::i18n::Translator) in its extensions
/// (typically injected by [`I18nLayer`](crate::i18n::I18nLayer)), the key is
/// resolved at response-build time and the translated string is used as the
/// response `message`. Otherwise the error's stored `message` is used
/// unchanged.
///
/// # Example
///
/// ```rust,no_run
/// use axum::{Router, routing::get};
/// use modo::middleware::{default_error_handler, error_handler};
///
/// let app: Router = Router::new()
///     .route("/", get(|| async { "ok" }))
///     .layer(error_handler(default_error_handler));
/// ```
pub async fn default_error_handler(err: crate::error::Error, parts: Parts) -> Response {
    let status = err.status();
    let details = err.details().cloned();

    let message = match (
        err.locale_key(),
        parts.extensions.get::<crate::i18n::Translator>(),
    ) {
        (Some(key), Some(tr)) => tr.t(key, &[]),
        _ => err.message().to_string(),
    };

    let body = render_error_body(status, &message, details.as_ref());
    (status, axum::Json(body)).into_response()
}

/// Tower [`Layer`] produced by [`error_handler`].
#[derive(Clone)]
pub struct ErrorHandlerLayer<F> {
    handler: F,
}

impl<S, F> Layer<S> for ErrorHandlerLayer<F>
where
    F: Clone,
{
    type Service = ErrorHandlerService<S, F>;

    fn layer(&self, inner: S) -> Self::Service {
        ErrorHandlerService {
            inner,
            handler: self.handler.clone(),
        }
    }
}

/// Tower [`Service`] that wraps an inner service and rewrites error responses
/// through a user-provided handler.
#[derive(Clone)]
pub struct ErrorHandlerService<S, F> {
    inner: S,
    handler: F,
}

impl<S, F, Fut> Service<http::Request<axum::body::Body>> for ErrorHandlerService<S, F>
where
    S: Service<http::Request<axum::body::Body>, Response = Response> + Clone + Send + 'static,
    S::Future: Send,
    S::Error: Into<std::convert::Infallible>,
    F: Fn(crate::error::Error, Parts) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = Response> + Send + 'static,
{
    type Response = Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Response, S::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: http::Request<axum::body::Body>) -> Self::Future {
        // Clone parts before consuming the request so the error handler can
        // inspect method, URI, headers, etc.
        let (parts, body) = req.into_parts();
        let saved_parts = parts.clone();
        let req = http::Request::from_parts(parts, body);

        let handler = self.handler.clone();
        let future = self.inner.call(req);

        Box::pin(async move {
            let response = future.await?;

            if let Some(error) = response.extensions().get::<crate::error::Error>() {
                let error = error.clone();
                let new_response = handler(error, saved_parts).await;
                Ok(new_response)
            } else {
                Ok(response)
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::Error as ModoError;
    use crate::i18n::{I18n, I18nConfig};
    use axum::body::Body;
    use axum::{Router, routing::get};
    use http::{Request, StatusCode};
    use tower::ServiceExt;

    fn test_i18n(dir: &std::path::Path) -> I18n {
        let en_dir = dir.join("en");
        let uk_dir = dir.join("uk");
        std::fs::create_dir_all(&en_dir).unwrap();
        std::fs::create_dir_all(&uk_dir).unwrap();
        std::fs::write(
            en_dir.join("errors.yaml"),
            "user:\n  not_found: User not found\n",
        )
        .unwrap();
        std::fs::write(
            uk_dir.join("errors.yaml"),
            "user:\n  not_found: Користувача не знайдено\n",
        )
        .unwrap();

        let config = I18nConfig {
            locales_path: dir.to_str().unwrap().to_string(),
            default_locale: "en".into(),
            ..I18nConfig::default()
        };
        I18n::new(&config).unwrap()
    }

    async fn decode_json(resp: axum::response::Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    async fn localized_handler() -> Result<&'static str, ModoError> {
        Err(ModoError::localized(
            StatusCode::NOT_FOUND,
            "errors.user.not_found",
        ))
    }

    async fn plain_handler() -> Result<&'static str, ModoError> {
        Err(ModoError::bad_request("boom"))
    }

    #[tokio::test]
    async fn default_handler_uses_translator_when_present() {
        let dir = tempfile::tempdir().unwrap();
        let i18n = test_i18n(dir.path());

        let app = Router::new()
            .route("/", get(localized_handler))
            .layer(error_handler(default_error_handler))
            .layer(i18n.layer());

        let req = Request::builder().uri("/").body(Body::empty()).unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        let body = decode_json(resp).await;
        assert_eq!(body["error"]["status"], 404);
        assert_eq!(body["error"]["message"], "User not found");
    }

    #[tokio::test]
    async fn default_handler_translates_using_resolved_locale() {
        let dir = tempfile::tempdir().unwrap();
        let i18n = test_i18n(dir.path());

        let app = Router::new()
            .route("/", get(localized_handler))
            .layer(error_handler(default_error_handler))
            .layer(i18n.layer());

        let req = Request::builder()
            .uri("/?lang=uk")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        let body = decode_json(resp).await;
        assert_eq!(body["error"]["message"], "Користувача не знайдено");
    }

    #[tokio::test]
    async fn default_handler_falls_back_to_key_without_translator() {
        // No I18nLayer is installed, so no Translator exists in the extensions.
        let app = Router::new()
            .route("/", get(localized_handler))
            .layer(error_handler(default_error_handler));

        let req = Request::builder().uri("/").body(Body::empty()).unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        let body = decode_json(resp).await;
        // Fallback is the raw translation key.
        assert_eq!(body["error"]["message"], "errors.user.not_found");
    }

    #[tokio::test]
    async fn default_handler_passes_through_plain_errors() {
        // With a Translator installed.
        let dir = tempfile::tempdir().unwrap();
        let i18n = test_i18n(dir.path());

        let app = Router::new()
            .route("/", get(plain_handler))
            .layer(error_handler(default_error_handler))
            .layer(i18n.layer());

        let req = Request::builder().uri("/").body(Body::empty()).unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let body = decode_json(resp).await;
        assert_eq!(body["error"]["message"], "boom");

        // And without one.
        let app = Router::new()
            .route("/", get(plain_handler))
            .layer(error_handler(default_error_handler));
        let req = Request::builder().uri("/").body(Body::empty()).unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let body = decode_json(resp).await;
        assert_eq!(body["error"]["message"], "boom");
    }
}
