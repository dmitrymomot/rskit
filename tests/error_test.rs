use http::StatusCode;
use modo::error::{Error, HttpError};

#[test]
fn test_error_creation() {
    let err = Error::new(StatusCode::NOT_FOUND, "not found");
    assert_eq!(err.status(), StatusCode::NOT_FOUND);
    assert_eq!(err.message(), "not found");
}

#[test]
fn test_error_helpers() {
    let err = Error::not_found("user not found");
    assert_eq!(err.status(), StatusCode::NOT_FOUND);

    let err = Error::bad_request("invalid input");
    assert_eq!(err.status(), StatusCode::BAD_REQUEST);

    let err = Error::internal("something broke");
    assert_eq!(err.status(), StatusCode::INTERNAL_SERVER_ERROR);

    let err = Error::unauthorized("not logged in");
    assert_eq!(err.status(), StatusCode::UNAUTHORIZED);

    let err = Error::forbidden("not allowed");
    assert_eq!(err.status(), StatusCode::FORBIDDEN);

    let err = Error::conflict("already exists");
    assert_eq!(err.status(), StatusCode::CONFLICT);
}

#[test]
fn test_http_error_to_error() {
    let err: Error = HttpError::NotFound.into();
    assert_eq!(err.status(), StatusCode::NOT_FOUND);

    let err: Error = HttpError::InternalServerError.into();
    assert_eq!(err.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[test]
fn test_error_from_io_error() {
    let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
    let err: Error = io_err.into();
    assert_eq!(err.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[test]
fn test_error_display() {
    let err = Error::not_found("user not found");
    assert_eq!(format!("{err}"), "user not found");
}

#[test]
fn test_error_into_response() {
    use axum::response::IntoResponse;
    let err = Error::not_found("missing");
    let response = err.into_response();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
