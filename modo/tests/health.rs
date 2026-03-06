use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::get;
use modo::health::{self, ReadinessCheck};
use std::sync::Arc;
use tower::ServiceExt;

#[tokio::test]
async fn test_liveness_200() {
    let app = Router::new().route("/_live", get(health::liveness_handler));

    let response = app
        .oneshot(
            Request::builder()
                .uri("/_live")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(&body[..], b"ok");
}

#[tokio::test]
async fn test_readiness_200_default() {
    // No checks registered — readiness handler with empty checks returns OK
    let checks: Vec<ReadinessCheck> = vec![];
    let checks_clone = checks.clone();
    let app = Router::new().route(
        "/_ready",
        get(move || {
            let checks = checks_clone.clone();
            async move { health::readiness_handler(checks).await }
        }),
    );

    let response = app
        .oneshot(
            Request::builder()
                .uri("/_ready")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(&body[..], b"ok");
}

#[tokio::test]
async fn test_readiness_500_on_failure() {
    let checks: Vec<ReadinessCheck> =
        vec![Arc::new(|| Box::pin(async { Err("db is down".into()) }))];
    let checks_clone = checks.clone();

    let app = Router::new().route(
        "/_ready",
        get(move || {
            let checks = checks_clone.clone();
            async move { health::readiness_handler(checks).await }
        }),
    );

    let response = app
        .oneshot(
            Request::builder()
                .uri("/_ready")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(&body[..], b"internal server error");
}
