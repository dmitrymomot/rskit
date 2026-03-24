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
/// The response includes a `modo::Error` in its extensions for downstream
/// middleware (such as [`error_handler`](super::error_handler)) to inspect.
pub fn catch_panic() -> CatchPanicLayer<ModoPanicHandler> {
    CatchPanicLayer::custom(ModoPanicHandler)
}
