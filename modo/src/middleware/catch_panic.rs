use crate::error::Error;
use axum::body::Body;
use axum::response::IntoResponse;
use std::any::Any;
use tower_http::catch_panic::ResponseForPanic;

/// Custom panic handler for the `CatchPanicLayer`.
///
/// Converts handler panics into structured `500 Internal Server Error`
/// JSON responses instead of aborting the connection.
#[derive(Clone)]
pub struct PanicHandler;

impl ResponseForPanic for PanicHandler {
    type ResponseBody = Body;

    fn response_for_panic(
        &mut self,
        err: Box<dyn Any + Send + 'static>,
    ) -> axum::http::Response<Body> {
        let msg = extract_panic_message(&err);
        tracing::error!(panic_message = %msg, "Handler panicked");
        Error::internal_panic(&msg).into_response()
    }
}

fn extract_panic_message(err: &Box<dyn Any + Send>) -> String {
    if let Some(s) = err.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = err.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown panic".to_string()
    }
}
