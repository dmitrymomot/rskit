use axum::http::StatusCode;
use axum::response::IntoResponse;
use rskit::error::RskitError;

#[test]
fn test_error_status_codes() {
    assert_eq!(RskitError::NotFound.status_code(), StatusCode::NOT_FOUND);
    assert_eq!(RskitError::Unauthorized.status_code(), StatusCode::UNAUTHORIZED);
    assert_eq!(RskitError::Forbidden.status_code(), StatusCode::FORBIDDEN);
    assert_eq!(
        RskitError::BadRequest("test".into()).status_code(),
        StatusCode::BAD_REQUEST,
    );
    assert_eq!(RskitError::RateLimited.status_code(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(
        RskitError::internal("oops").status_code(),
        StatusCode::INTERNAL_SERVER_ERROR,
    );
}

#[tokio::test]
async fn test_error_json_response() {
    let err = RskitError::NotFound;
    let response = err.into_response();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[test]
fn test_from_anyhow() {
    let anyhow_err = anyhow::anyhow!("something went wrong");
    let rskit_err: RskitError = anyhow_err.into();
    assert_eq!(rskit_err.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
    assert!(rskit_err.to_string().contains("something went wrong"));
}

#[test]
fn test_error_display() {
    assert_eq!(RskitError::NotFound.to_string(), "Not found");
    assert_eq!(RskitError::Unauthorized.to_string(), "Unauthorized");
    assert_eq!(RskitError::Forbidden.to_string(), "Forbidden");
    assert_eq!(
        RskitError::BadRequest("invalid input".into()).to_string(),
        "Bad request: invalid input",
    );
    assert_eq!(RskitError::RateLimited.to_string(), "Rate limited");
}
