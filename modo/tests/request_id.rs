use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::get;
use modo::request_id::{RequestId, request_id_middleware};
use tower::ServiceExt;

fn build_test_router() -> Router {
    Router::new()
        .route(
            "/echo",
            get(|req_id: axum::Extension<RequestId>| async move { req_id.0.to_string() }),
        )
        .layer(axum::middleware::from_fn(request_id_middleware))
}

#[tokio::test]
async fn test_generates_request_id() {
    let app = build_test_router();

    let response = app
        .oneshot(Request::builder().uri("/echo").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let header = response
        .headers()
        .get("x-request-id")
        .expect("x-request-id header missing");
    let id = header.to_str().unwrap();
    // ULID is 26 chars lowercase
    assert_eq!(id.len(), 26);
    assert!(id.chars().all(|c| c.is_ascii_alphanumeric()));
}

#[tokio::test]
async fn test_propagates_existing_id() {
    let app = build_test_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/echo")
                .header("x-request-id", "custom-123")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let header = response
        .headers()
        .get("x-request-id")
        .unwrap()
        .to_str()
        .unwrap();
    assert_eq!(header, "custom-123");

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(&body[..], b"custom-123");
}

#[tokio::test]
async fn test_ignores_empty_header() {
    let app = build_test_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/echo")
                .header("x-request-id", "")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let header = response
        .headers()
        .get("x-request-id")
        .unwrap()
        .to_str()
        .unwrap();
    // Should be a generated ULID, not empty
    assert_eq!(header.len(), 26);
    assert!(header.chars().all(|c| c.is_ascii_alphanumeric()));
}
