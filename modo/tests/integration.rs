use axum::body::Body;
use axum::http::{Request, StatusCode};
use modo::app::AppState;
use modo::config::ServerConfig;
use modo::error::HttpError;
use modo::router::RouteRegistration;
use tower::ServiceExt;

#[modo::handler(GET, "/test")]
async fn test_handler() -> &'static str {
    "test response"
}

#[modo::handler(GET, "/test/error")]
async fn test_error() -> Result<&'static str, HttpError> {
    Err(HttpError::NotFound)
}

fn build_test_router() -> axum::Router {
    let state = AppState {
        services: Default::default(),
        server_config: ServerConfig::default(),
        cookie_key: axum_extra::extract::cookie::Key::generate(),
    };

    let mut router = axum::Router::new();
    for reg in inventory::iter::<RouteRegistration> {
        if reg.path.starts_with("/test") {
            let method_router = (reg.handler)();
            router = router.route(reg.path, method_router);
        }
    }
    router.with_state(state)
}

#[tokio::test]
async fn test_get_handler_returns_200() {
    let app = build_test_router();

    let response = app
        .oneshot(Request::builder().uri("/test").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(&body[..], b"test response");
}

#[tokio::test]
async fn test_error_handler_returns_404_json() {
    let app = build_test_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/test/error")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], 404);
    assert_eq!(json["error"], "not_found");
    assert_eq!(json["message"], "Not found");
}

#[tokio::test]
async fn test_unknown_route_returns_404() {
    let app = build_test_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/nonexistent")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
