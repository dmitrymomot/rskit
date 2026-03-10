use modo::error::{Error, ErrorContext, ErrorHandlerRegistration, HttpError};

#[modo::error_handler]
fn custom_error_handler(err: Error, _ctx: &ErrorContext) -> axum::response::Response {
    err.default_response()
}

#[test]
fn test_error_handler_registered() {
    let count = inventory::iter::<ErrorHandlerRegistration>().count();
    assert!(count > 0, "no error handler registered");
}

#[test]
fn test_error_handler_callable() {
    let reg = inventory::iter::<ErrorHandlerRegistration>()
        .next()
        .expect("no error handler registered");

    let error = Error::from(HttpError::NotFound);
    let ctx = ErrorContext {
        method: http::Method::GET,
        uri: "/test".parse().unwrap(),
        headers: http::HeaderMap::new(),
    };

    let response = (reg.handler)(error, &ctx);
    assert_eq!(response.status(), http::StatusCode::NOT_FOUND);
}
