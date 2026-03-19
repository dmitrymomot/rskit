use axum::Router;
use axum::body::Body;
use axum::http::Request;
use axum::routing::get;
use http::StatusCode;
use modo::sanitize::Sanitize;
use modo::service::Registry;
use serde::Deserialize;
use tower::ServiceExt;

#[tokio::test]
async fn test_service_extractor_success() {
    #[derive(Debug)]
    struct Greeter(String);

    async fn handler(modo::Service(greeter): modo::Service<Greeter>) -> String {
        greeter.0.clone()
    }

    let mut registry = Registry::new();
    registry.add(Greeter("hello".to_string()));
    let app = Router::new()
        .route("/", get(handler))
        .with_state(registry.into_state());

    let response = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(&body[..], b"hello");
}

#[tokio::test]
async fn test_service_extractor_missing_returns_500() {
    #[derive(Debug)]
    struct Missing;

    async fn handler(_: modo::Service<Missing>) -> String {
        "unreachable".to_string()
    }

    let registry = Registry::new();
    let app = Router::new()
        .route("/", get(handler))
        .with_state(registry.into_state());

    let response = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[derive(Deserialize)]
struct CreateItem {
    title: String,
}

impl Sanitize for CreateItem {
    fn sanitize(&mut self) {
        modo::sanitize::trim(&mut self.title);
    }
}

#[tokio::test]
async fn test_json_request_deserializes_and_sanitizes() {
    async fn handler(
        modo::extractor::JsonRequest(item): modo::extractor::JsonRequest<CreateItem>,
    ) -> String {
        item.title
    }

    let registry = Registry::new();
    let app = Router::new()
        .route("/", axum::routing::post(handler))
        .with_state(registry.into_state());

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"title":"  hello  "}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(&body[..], b"hello");
}

#[tokio::test]
async fn test_form_request_deserializes_and_sanitizes() {
    async fn handler(
        modo::extractor::FormRequest(item): modo::extractor::FormRequest<CreateItem>,
    ) -> String {
        item.title
    }

    let registry = Registry::new();
    let app = Router::new()
        .route("/", axum::routing::post(handler))
        .with_state(registry.into_state());

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from("title=%20+hello+%20"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(&body[..], b"hello");
}

#[tokio::test]
async fn test_query_extractor_sanitizes() {
    async fn handler(modo::extractor::Query(item): modo::extractor::Query<CreateItem>) -> String {
        item.title
    }

    let registry = Registry::new();
    let app = Router::new()
        .route("/", get(handler))
        .with_state(registry.into_state());

    let response = app
        .oneshot(
            Request::builder()
                .uri("/?title=%20+hello+%20")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(&body[..], b"hello");
}
