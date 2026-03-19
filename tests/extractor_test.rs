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

#[tokio::test]
async fn test_multipart_request_text_fields() {
    #[derive(Deserialize)]
    struct ProfileData {
        name: String,
    }
    impl Sanitize for ProfileData {
        fn sanitize(&mut self) {
            modo::sanitize::trim(&mut self.name);
        }
    }

    async fn handler(
        modo::extractor::MultipartRequest(data, _files): modo::extractor::MultipartRequest<
            ProfileData,
        >,
    ) -> String {
        data.name
    }

    let registry = Registry::new();
    let app = Router::new()
        .route("/", axum::routing::post(handler))
        .with_state(registry.into_state());

    let boundary = "----TestBoundary";
    let body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"name\"\r\n\r\n  Alice  \r\n--{boundary}--\r\n"
    );

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/")
                .header(
                    "content-type",
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(&body[..], b"Alice");
}

#[test]
fn test_uploaded_file_struct() {
    let file = modo::extractor::UploadedFile {
        name: "photo.jpg".to_string(),
        content_type: "image/jpeg".to_string(),
        size: 1024,
        data: bytes::Bytes::from_static(b"fake image data"),
    };
    assert_eq!(file.name, "photo.jpg");
    assert_eq!(file.size, 1024);
}

#[test]
fn test_files_get_and_file() {
    use std::collections::HashMap;

    let file = modo::extractor::UploadedFile {
        name: "doc.pdf".to_string(),
        content_type: "application/pdf".to_string(),
        size: 512,
        data: bytes::Bytes::from_static(b"pdf data"),
    };

    let mut map = HashMap::new();
    map.insert("document".to_string(), vec![file]);
    let mut files = modo::extractor::Files::from_map(map);

    assert!(files.get("document").is_some());
    assert!(files.get("missing").is_none());

    let taken = files.file("document").unwrap();
    assert_eq!(taken.name, "doc.pdf");
    assert!(files.get("document").is_none()); // removed after file()
}

#[tokio::test]
async fn test_json_request_rejects_invalid_json() {
    async fn handler(
        modo::extractor::JsonRequest(_item): modo::extractor::JsonRequest<CreateItem>,
    ) -> String {
        "unreachable".to_string()
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
                .body(Body::from("not json"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_multipart_request_with_file_upload() {
    #[derive(Deserialize)]
    struct UploadForm {
        name: String,
    }
    impl Sanitize for UploadForm {
        fn sanitize(&mut self) {
            modo::sanitize::trim(&mut self.name);
        }
    }

    async fn handler(
        modo::extractor::MultipartRequest(data, mut files): modo::extractor::MultipartRequest<
            UploadForm,
        >,
    ) -> String {
        let file = files.file("avatar").unwrap();
        format!(
            "{}|{}|{}|{}",
            data.name, file.name, file.content_type, file.size
        )
    }

    let registry = Registry::new();
    let app = Router::new()
        .route("/", axum::routing::post(handler))
        .with_state(registry.into_state());

    let boundary = "----TestFileBoundary";
    let file_data = b"fake image bytes";
    let body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"name\"\r\n\r\nAlice\r\n--{boundary}\r\nContent-Disposition: form-data; name=\"avatar\"; filename=\"photo.jpg\"\r\nContent-Type: image/jpeg\r\n\r\n{}\r\n--{boundary}--\r\n",
        String::from_utf8_lossy(file_data)
    );

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/")
                .header(
                    "content-type",
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let text = String::from_utf8_lossy(&body);
    assert_eq!(
        text,
        format!("Alice|photo.jpg|image/jpeg|{}", file_data.len())
    );
}
