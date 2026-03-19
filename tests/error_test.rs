use http::StatusCode;
use modo::error::{Error, HttpError};
use serial_test::serial;
use std::env;
use std::io::Write;

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

// --- New tests: core gaps ---

#[test]
fn test_error_with_source() {
    let io_err = std::io::Error::other("disk full");
    let err = Error::with_source(StatusCode::INTERNAL_SERVER_ERROR, "wrapped", io_err);
    assert_eq!(err.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(err.message(), "wrapped");
    assert!(std::error::Error::source(&err).is_some());
}

#[tokio::test]
async fn test_error_into_response_body() {
    use axum::response::IntoResponse;

    let err = Error::not_found("item missing");
    let response = err.into_response();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["error"]["status"], 404);
    assert_eq!(body["error"]["message"], "item missing");
}

#[test]
fn test_error_source_none_without_source() {
    let err = Error::new(StatusCode::OK, "ok");
    assert!(std::error::Error::source(&err).is_none());
}

#[test]
fn test_error_source_chain() {
    let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file gone");
    let err = Error::with_source(StatusCode::INTERNAL_SERVER_ERROR, "wrapped", io_err);
    let source = std::error::Error::source(&err).unwrap();
    assert!(
        format!("{source}").contains("file gone"),
        "expected source to contain 'file gone', got: {source}"
    );
}

#[test]
fn test_error_debug_format() {
    let err = Error::not_found("test debug");
    let debug = format!("{err:?}");
    assert!(
        debug.contains("Error"),
        "debug should contain 'Error': {debug}"
    );
    assert!(debug.contains("404"), "debug should contain '404': {debug}");
    assert!(
        debug.contains("test debug"),
        "debug should contain 'test debug': {debug}"
    );
}

// --- New tests: remaining helper constructors ---

#[test]
fn test_error_unprocessable_entity() {
    let err = Error::unprocessable_entity("invalid data");
    assert_eq!(err.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(err.message(), "invalid data");
}

#[test]
fn test_error_too_many_requests() {
    let err = Error::too_many_requests("slow down");
    assert_eq!(err.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(err.message(), "slow down");
}

// --- New tests: HttpError all variants ---

#[test]
fn test_http_error_all_variants_status_and_message() {
    let cases: &[(HttpError, StatusCode, &str)] = &[
        (
            HttpError::BadRequest,
            StatusCode::BAD_REQUEST,
            "Bad Request",
        ),
        (
            HttpError::Unauthorized,
            StatusCode::UNAUTHORIZED,
            "Unauthorized",
        ),
        (HttpError::Forbidden, StatusCode::FORBIDDEN, "Forbidden"),
        (HttpError::NotFound, StatusCode::NOT_FOUND, "Not Found"),
        (
            HttpError::MethodNotAllowed,
            StatusCode::METHOD_NOT_ALLOWED,
            "Method Not Allowed",
        ),
        (HttpError::Conflict, StatusCode::CONFLICT, "Conflict"),
        (HttpError::Gone, StatusCode::GONE, "Gone"),
        (
            HttpError::UnprocessableEntity,
            StatusCode::UNPROCESSABLE_ENTITY,
            "Unprocessable Entity",
        ),
        (
            HttpError::TooManyRequests,
            StatusCode::TOO_MANY_REQUESTS,
            "Too Many Requests",
        ),
        (
            HttpError::InternalServerError,
            StatusCode::INTERNAL_SERVER_ERROR,
            "Internal Server Error",
        ),
        (
            HttpError::BadGateway,
            StatusCode::BAD_GATEWAY,
            "Bad Gateway",
        ),
        (
            HttpError::ServiceUnavailable,
            StatusCode::SERVICE_UNAVAILABLE,
            "Service Unavailable",
        ),
        (
            HttpError::GatewayTimeout,
            StatusCode::GATEWAY_TIMEOUT,
            "Gateway Timeout",
        ),
    ];

    for (variant, expected_status, expected_message) in cases {
        assert_eq!(
            variant.status_code(),
            *expected_status,
            "status mismatch for {expected_message}"
        );
        assert_eq!(
            variant.message(),
            *expected_message,
            "message mismatch for {expected_message}"
        );
    }
}

#[test]
fn test_http_error_all_variants_into_error() {
    let cases: &[(HttpError, StatusCode, &str)] = &[
        (
            HttpError::BadRequest,
            StatusCode::BAD_REQUEST,
            "Bad Request",
        ),
        (
            HttpError::Unauthorized,
            StatusCode::UNAUTHORIZED,
            "Unauthorized",
        ),
        (HttpError::Forbidden, StatusCode::FORBIDDEN, "Forbidden"),
        (HttpError::NotFound, StatusCode::NOT_FOUND, "Not Found"),
        (
            HttpError::MethodNotAllowed,
            StatusCode::METHOD_NOT_ALLOWED,
            "Method Not Allowed",
        ),
        (HttpError::Conflict, StatusCode::CONFLICT, "Conflict"),
        (HttpError::Gone, StatusCode::GONE, "Gone"),
        (
            HttpError::UnprocessableEntity,
            StatusCode::UNPROCESSABLE_ENTITY,
            "Unprocessable Entity",
        ),
        (
            HttpError::TooManyRequests,
            StatusCode::TOO_MANY_REQUESTS,
            "Too Many Requests",
        ),
        (
            HttpError::InternalServerError,
            StatusCode::INTERNAL_SERVER_ERROR,
            "Internal Server Error",
        ),
        (
            HttpError::BadGateway,
            StatusCode::BAD_GATEWAY,
            "Bad Gateway",
        ),
        (
            HttpError::ServiceUnavailable,
            StatusCode::SERVICE_UNAVAILABLE,
            "Service Unavailable",
        ),
        (
            HttpError::GatewayTimeout,
            StatusCode::GATEWAY_TIMEOUT,
            "Gateway Timeout",
        ),
    ];

    for (variant, expected_status, expected_message) in cases {
        let err: Error = (*variant).into();
        assert_eq!(
            err.status(),
            *expected_status,
            "status mismatch for {expected_message}"
        );
        assert_eq!(
            err.message(),
            *expected_message,
            "message mismatch for {expected_message}"
        );
    }
}

// --- New tests: From impls for external errors ---

#[test]
fn test_error_from_serde_json() {
    let json_err = modo::serde_json::from_str::<modo::serde_json::Value>("{{invalid")
        .expect_err("should fail to parse");
    let err: Error = json_err.into();
    assert_eq!(err.status(), StatusCode::BAD_REQUEST);
    assert_eq!(err.message(), "JSON error");
    assert!(std::error::Error::source(&err).is_some());
}

#[test]
#[serial]
fn test_error_from_yaml_via_config_load() {
    let dir = tempfile::tempdir().unwrap();
    let config_dir = dir.path();

    let yaml_path = config_dir.join("test.yaml");
    let mut f = std::fs::File::create(&yaml_path).unwrap();
    writeln!(f, ":\n  - :\n  [{{").unwrap();

    unsafe { env::set_var("APP_ENV", "test") };
    let result = modo::config::load::<modo::serde_json::Value>(config_dir.to_str().unwrap());
    unsafe { env::remove_var("APP_ENV") };

    let err = result.expect_err("should fail to parse invalid YAML");
    assert_eq!(err.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(err.message(), "YAML error");
}

// --- New tests: Error.details ---

#[test]
fn test_error_with_details() {
    let err =
        modo::Error::unprocessable_entity("validation failed").with_details(serde_json::json!({
            "title": ["must be at least 3 characters"]
        }));
    assert_eq!(err.status(), http::StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(err.message(), "validation failed");
    let details = err.details().unwrap();
    assert_eq!(details["title"][0], "must be at least 3 characters");
}

#[test]
fn test_error_without_details() {
    let err = modo::Error::not_found("missing");
    assert!(err.details().is_none());
}

#[test]
fn test_error_with_details_into_response() {
    use axum::response::IntoResponse;
    let err = modo::Error::unprocessable_entity("validation failed")
        .with_details(serde_json::json!({"title": ["too short"]}));
    let response = err.into_response();
    assert_eq!(response.status(), http::StatusCode::UNPROCESSABLE_ENTITY);
}
