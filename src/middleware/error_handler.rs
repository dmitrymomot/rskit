use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use axum::response::Response;
use http::request::Parts;
use tower::{Layer, Service};

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
