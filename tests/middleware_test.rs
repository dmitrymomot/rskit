use axum::Router;
use axum::body::Body;
use axum::http::Request;
use axum::routing::get;
use http::StatusCode;
use modo::service::Registry;
use tower::ServiceExt;

#[tokio::test]
async fn test_request_id_sets_header() {
    async fn handler() -> &'static str {
        "ok"
    }

    let app = Router::new()
        .route("/", get(handler))
        .layer(modo::middleware::request_id())
        .with_state(Registry::new().into_state());

    let response = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let request_id = response.headers().get("x-request-id");
    assert!(request_id.is_some());
    assert_eq!(request_id.unwrap().len(), 26); // ULID length
}

#[tokio::test]
async fn test_request_id_preserves_existing() {
    async fn handler() -> &'static str {
        "ok"
    }

    let app = Router::new()
        .route("/", get(handler))
        .layer(modo::middleware::request_id())
        .with_state(Registry::new().into_state());

    let response = app
        .oneshot(
            Request::builder()
                .uri("/")
                .header("x-request-id", "existing-id")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let request_id = response.headers().get("x-request-id");
    assert!(request_id.is_some());
    assert_eq!(request_id.unwrap().to_str().unwrap(), "existing-id");
}

#[tokio::test]
async fn test_compression_layer_compiles() {
    async fn handler() -> &'static str {
        "ok"
    }

    let app = Router::new()
        .route("/", get(handler))
        .layer(modo::middleware::compression())
        .with_state(Registry::new().into_state());

    let response = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_tracing_layer_compiles() {
    async fn handler() -> &'static str {
        "ok"
    }

    let app = Router::new()
        .route("/", get(handler))
        .layer(modo::middleware::tracing())
        .with_state(Registry::new().into_state());

    let response = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_catch_panic_returns_500() {
    async fn panicking_handler() -> &'static str {
        panic!("boom");
    }

    let app = Router::new()
        .route("/", get(panicking_handler))
        .layer(modo::middleware::catch_panic())
        .with_state(Registry::new().into_state());

    let response = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    // Verify modo::Error is in response extensions
    let error = response.extensions().get::<modo::Error>();
    assert!(error.is_some());
}
