//! Integration test for max_file_size enforcement in MultipartForm extractor.
//!
//! The `MultipartForm<T>` extractor reads `UploadConfig` from
//! `AppState.services.get::<UploadConfig>()` and passes `max_file_size` to
//! `FromMultipart::from_multipart()`. When a file exceeds the global
//! max_file_size, it returns 413 PAYLOAD_TOO_LARGE. When within the limit,
//! it returns 200 OK.

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::post;
use modo::app::{AppState, ServiceRegistry};
use modo::config::ServerConfig;
use modo_upload::{FromMultipart, MultipartForm, UploadConfig, UploadedFile};
use tower::ServiceExt;

#[derive(FromMultipart)]
struct TestUpload {
    #[allow(dead_code)]
    file: UploadedFile,
}

async fn upload_handler(form: MultipartForm<TestUpload>) -> &'static str {
    let _ = form.into_inner();
    "ok"
}

fn app_state_with_max_file_size(max_size: &str) -> AppState {
    let config = UploadConfig {
        max_file_size: Some(max_size.to_string()),
        ..Default::default()
    };
    let services = ServiceRegistry::new().with(config);
    AppState {
        services,
        server_config: ServerConfig::default(),
        cookie_key: axum_extra::extract::cookie::Key::generate(),
    }
}

fn multipart_request(file_content: &[u8]) -> Request<Body> {
    let boundary = "----TestBoundary";
    let mut body = format!(
        "--{boundary}\r\n\
         Content-Disposition: form-data; name=\"file\"; filename=\"test.bin\"\r\n\
         Content-Type: application/octet-stream\r\n\r\n"
    )
    .into_bytes();
    body.extend_from_slice(file_content);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());

    Request::builder()
        .method("POST")
        .uri("/upload")
        .header(
            "content-type",
            format!("multipart/form-data; boundary={boundary}"),
        )
        .body(Body::from(body))
        .unwrap()
}

#[tokio::test]
async fn rejects_file_exceeding_max_file_size() {
    let state = app_state_with_max_file_size("100b");
    let app = Router::new()
        .route("/upload", post(upload_handler))
        .with_state(state);

    // Send 200 bytes, limit is 100 bytes
    let request = multipart_request(&[0u8; 200]);
    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn accepts_file_within_max_file_size() {
    let state = app_state_with_max_file_size("1kb");
    let app = Router::new()
        .route("/upload", post(upload_handler))
        .with_state(state);

    // Send 100 bytes, limit is 1024 bytes
    let request = multipart_request(&[0u8; 100]);
    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}
