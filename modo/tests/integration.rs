use axum::body::Body;
use axum::http::{Request, StatusCode};
use modo::router::RouteRegistration;
use modo::{AppState, HttpError, ServerConfig};
use tower::ServiceExt;

#[modo::handler(GET, "/test")]
async fn test_handler() -> &'static str {
    "test response"
}

#[modo::handler(GET, "/test/error")]
async fn test_error() -> Result<&'static str, HttpError> {
    Err(HttpError::NotFound)
}

#[modo::handler(GET, "/test/items/{id}")]
async fn test_path_param(id: String) -> String {
    format!("item:{id}")
}

#[modo::handler(GET, "/test/users/{user_id}/posts/{post_id}")]
async fn test_partial_path_params(post_id: String) -> String {
    format!("post:{post_id}")
}

#[modo::handler(GET, "/test/a/{x}/b/{y}")]
async fn test_all_path_params(x: String, y: String) -> String {
    format!("{x}:{y}")
}

fn build_test_router() -> axum::Router {
    use axum::response::IntoResponse;

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
    router
        .fallback(|| async { HttpError::NotFound.into_response() })
        .with_state(state)
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

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], 404);
    assert_eq!(json["error"], "not_found");
}

#[tokio::test]
async fn test_single_path_param() {
    let app = build_test_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/test/items/42")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(&body[..], b"item:42");
}

#[tokio::test]
async fn test_partial_path_param_extraction() {
    let app = build_test_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/test/users/u1/posts/p2")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(&body[..], b"post:p2");
}

#[tokio::test]
async fn test_all_path_params_declared() {
    let app = build_test_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/test/a/hello/b/world")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(&body[..], b"hello:world");
}
