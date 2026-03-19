use std::any::Any;

use axum::response::{IntoResponse, Response};
use http::StatusCode;
use tower_http::catch_panic::CatchPanicLayer;

/// Panic handler used internally by [`catch_panic`].
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
/// The response includes a `modo::Error` in its extensions for downstream middleware
/// to inspect.
pub fn catch_panic() -> CatchPanicLayer<ModoPanicHandler> {
    CatchPanicLayer::custom(ModoPanicHandler)
}
