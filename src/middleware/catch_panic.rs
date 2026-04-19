use std::any::Any;

use axum::response::{IntoResponse, Response};
use http::StatusCode;
use tower_http::catch_panic::CatchPanicLayer;

/// Panic handler used internally by [`catch_panic`].
///
/// Converts a caught panic into a `500 Internal Server Error` response and
/// stores a `modo::Error` in the response extensions so that
/// [`error_handler`](super::error_handler) can rewrite it.
#[derive(Clone)]
pub struct ModoPanicHandler;

impl tower_http::catch_panic::ResponseForPanic for ModoPanicHandler {
    type ResponseBody = axum::body::Body;

    fn response_for_panic(
        &mut self,
        _err: Box<dyn Any + Send + 'static>,
    ) -> Response<Self::ResponseBody> {
        let error = crate::error::Error::internal("internal server error");
        let mut response = StatusCode::INTERNAL_SERVER_ERROR.into_response();
        response.extensions_mut().insert(error);
        response
    }
}

/// Returns a layer that catches panics in handlers and returns a 500 response.
///
/// The response is a bare `500 Internal Server Error` with a
/// [`crate::Error::internal`] value in its extensions, so
/// [`error_handler`](super::error_handler) can rewrite the body through
/// the application's chosen responder.
///
/// # Layer ordering
///
/// Install `catch_panic()` **outside** the handler (so it sees handler
/// panics) but **inside** [`tracing`](super::tracing) so the panic is
/// still observed in a span. If installed *inside* `error_handler` the
/// 500 response is re-rendered by the handler; if installed *outside* it
/// the raw 500 bypasses the handler.
///
/// # Example
///
/// ```rust,no_run
/// use axum::{Router, routing::get};
/// use modo::middleware::catch_panic;
///
/// let app: Router = Router::new()
///     .route("/", get(|| async { "ok" }))
///     .layer(catch_panic());
/// ```
pub fn catch_panic() -> CatchPanicLayer<ModoPanicHandler> {
    CatchPanicLayer::custom(ModoPanicHandler)
}
