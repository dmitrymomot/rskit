use axum::http::StatusCode;
use axum::response::IntoResponse;
use modo::error::Error;

#[test]
fn test_error_status_codes() {
    assert_eq!(Error::NotFound.status_code(), StatusCode::NOT_FOUND);
    assert_eq!(Error::Unauthorized.status_code(), StatusCode::UNAUTHORIZED);
    assert_eq!(Error::Forbidden.status_code(), StatusCode::FORBIDDEN);
    assert_eq!(
        Error::BadRequest("test".into()).status_code(),
        StatusCode::BAD_REQUEST,
    );
    assert_eq!(
        Error::RateLimited.status_code(),
        StatusCode::TOO_MANY_REQUESTS
    );
    assert_eq!(
        Error::internal("oops").status_code(),
        StatusCode::INTERNAL_SERVER_ERROR,
    );
}

#[tokio::test]
async fn test_error_json_response() {
    let err = Error::NotFound;
    let response = err.into_response();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[test]
fn test_from_anyhow() {
    let anyhow_err = anyhow::anyhow!("something went wrong");
    let modo_err: Error = anyhow_err.into();
    assert_eq!(modo_err.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
    assert!(modo_err.to_string().contains("something went wrong"));
}

#[test]
fn test_error_display() {
    assert_eq!(Error::NotFound.to_string(), "Not found");
    assert_eq!(Error::Unauthorized.to_string(), "Unauthorized");
    assert_eq!(Error::Forbidden.to_string(), "Forbidden");
    assert_eq!(
        Error::BadRequest("invalid input".into()).to_string(),
        "Bad request: invalid input",
    );
    assert_eq!(Error::RateLimited.to_string(), "Rate limited");
}
