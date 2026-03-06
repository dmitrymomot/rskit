use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use modo::error::{Error, ErrorContext, HttpError};
use serde_json::json;

#[test]
fn test_http_error_status_codes() {
    assert_eq!(HttpError::BadRequest.status_code(), StatusCode::BAD_REQUEST);
    assert_eq!(
        HttpError::Unauthorized.status_code(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(HttpError::Forbidden.status_code(), StatusCode::FORBIDDEN);
    assert_eq!(HttpError::NotFound.status_code(), StatusCode::NOT_FOUND);
    assert_eq!(HttpError::Conflict.status_code(), StatusCode::CONFLICT);
    assert_eq!(
        HttpError::UnprocessableEntity.status_code(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        HttpError::TooManyRequests.status_code(),
        StatusCode::TOO_MANY_REQUESTS
    );
    assert_eq!(HttpError::ImATeapot.status_code(), StatusCode::IM_A_TEAPOT);
    assert_eq!(
        HttpError::InternalServerError.status_code(),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(
        HttpError::ServiceUnavailable.status_code(),
        StatusCode::SERVICE_UNAVAILABLE
    );
}

#[test]
fn test_http_error_code_and_message() {
    assert_eq!(HttpError::NotFound.code(), "not_found");
    assert_eq!(HttpError::NotFound.message(), "Not found");
    assert_eq!(HttpError::BadRequest.code(), "bad_request");
    assert_eq!(HttpError::BadRequest.message(), "Bad request");
    assert_eq!(
        HttpError::InternalServerError.code(),
        "internal_server_error"
    );
    assert_eq!(
        HttpError::InternalServerError.message(),
        "Internal server error"
    );
}

#[test]
fn test_error_from_http_error() {
    let err = Error::from(HttpError::NotFound);
    assert_eq!(err.status_code(), StatusCode::NOT_FOUND);
    assert_eq!(err.code(), "not_found");
}

#[test]
fn test_error_display() {
    let err = Error::from(HttpError::NotFound);
    assert_eq!(err.to_string(), "not_found: Not found");

    let err = HttpError::BadRequest.with_message("Validation failed");
    assert_eq!(err.to_string(), "bad_request: Validation failed");
}

#[test]
fn test_error_internal_convenience() {
    let err = Error::internal("DB connection failed");
    assert_eq!(err.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(err.code(), "internal_server_error");
    assert_eq!(
        err.to_string(),
        "internal_server_error: DB connection failed"
    );
}

#[test]
fn test_error_with_message() {
    let err = HttpError::NotFound.with_message("User not found");
    assert_eq!(err.status_code(), StatusCode::NOT_FOUND);
    assert_eq!(err.to_string(), "not_found: User not found");
}

#[test]
fn test_error_detail_builder() {
    let err = HttpError::BadRequest
        .with_message("Validation failed")
        .detail(
            "email",
            json!(["email is required", "must be valid email address"]),
        )
        .detail("name", json!(["too short"]))
        .detail("retry_after", json!(30));

    assert_eq!(err.status_code(), StatusCode::BAD_REQUEST);
    assert_eq!(err.code(), "bad_request");
}

#[test]
fn test_error_new() {
    let err = Error::new(StatusCode::IM_A_TEAPOT, "teapot", "I'm a teapot");
    assert_eq!(err.status_code(), StatusCode::IM_A_TEAPOT);
    assert_eq!(err.code(), "teapot");
    assert_eq!(err.to_string(), "teapot: I'm a teapot");
}

#[test]
fn test_from_anyhow() {
    let anyhow_err = anyhow::anyhow!("something went wrong");
    let err: Error = anyhow_err.into();
    assert_eq!(err.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(err.code(), "internal_server_error");
    assert!(err.to_string().contains("something went wrong"));
}

#[tokio::test]
async fn test_4xx_response_includes_details() {
    let err = HttpError::BadRequest
        .with_message("Validation failed")
        .detail("email", json!(["email is required"]));

    let response = err.into_response();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["error"], "bad_request");
    assert_eq!(json["message"], "Validation failed");
    assert_eq!(json["status"], 400);
    assert_eq!(json["details"]["email"], json!(["email is required"]));
}

#[tokio::test]
async fn test_5xx_response_hides_details() {
    let err =
        Error::internal("secret DB password exposed").detail("debug", json!("should not leak"));

    let response = err.into_response();
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["error"], "internal_server_error");
    assert_eq!(json["message"], "Internal server error");
    assert_eq!(json["status"], 500);
    assert_eq!(json["details"], json!({}));
}

#[tokio::test]
async fn test_http_error_into_response() {
    let response = HttpError::NotFound.into_response();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["error"], "not_found");
    assert_eq!(json["message"], "Not found");
    assert_eq!(json["status"], 404);
    assert_eq!(json["details"], json!({}));
}

#[test]
fn test_error_source_trait() {
    use std::error::Error as StdError;
    let err = Error::from(HttpError::NotFound);
    assert!(StdError::source(&err).is_none());

    let anyhow_err = anyhow::anyhow!("root cause");
    let err: Error = anyhow_err.into();
    assert!(StdError::source(&err).is_some());
}

#[test]
fn test_error_message_builder() {
    let err = Error::from(HttpError::NotFound).message("Custom message");
    assert_eq!(err.to_string(), "not_found: Custom message");
}

#[test]
fn test_error_message_str_getter() {
    let err = HttpError::NotFound.with_message("User not found");
    assert_eq!(err.message_str(), "User not found");
}

#[test]
fn test_error_details_getter() {
    let err = HttpError::BadRequest
        .with_message("Validation failed")
        .detail("email", json!(["required"]));
    let details = err.details();
    assert_eq!(details.len(), 1);
    assert_eq!(details["email"], json!(["required"]));
}

#[tokio::test]
async fn test_default_response_renders_json() {
    let err = HttpError::NotFound.with_message("User not found");
    let response = err.default_response();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error"], "not_found");
    assert_eq!(json["message"], "User not found");
    assert_eq!(json["status"], 404);
}

#[tokio::test]
async fn test_into_response_attaches_error_extension() {
    let err = Error::from(HttpError::NotFound);
    let response = err.into_response();

    assert!(response.extensions().get::<Error>().is_some());
    let ext = response.extensions().get::<Error>().unwrap();
    assert_eq!(ext.status_code(), StatusCode::NOT_FOUND);
    assert_eq!(ext.code(), "not_found");
}

#[test]
fn test_error_context_accepts_html() {
    let mut headers = HeaderMap::new();
    headers.insert("accept", "text/html,application/json".parse().unwrap());
    let ctx = ErrorContext {
        method: axum::http::Method::GET,
        uri: "/test".parse().unwrap(),
        headers,
    };
    assert!(ctx.accepts_html());

    let ctx_json = ErrorContext {
        method: axum::http::Method::GET,
        uri: "/test".parse().unwrap(),
        headers: HeaderMap::new(),
    };
    assert!(!ctx_json.accepts_html());
}

#[test]
fn test_error_context_is_htmx() {
    let mut headers = HeaderMap::new();
    headers.insert("hx-request", "true".parse().unwrap());
    let ctx = ErrorContext {
        method: axum::http::Method::GET,
        uri: "/test".parse().unwrap(),
        headers,
    };
    assert!(ctx.is_htmx());

    let ctx_no_htmx = ErrorContext {
        method: axum::http::Method::GET,
        uri: "/test".parse().unwrap(),
        headers: HeaderMap::new(),
    };
    assert!(!ctx_no_htmx.is_htmx());
}
