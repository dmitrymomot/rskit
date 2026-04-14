#![cfg(feature = "test-helpers")]

use axum::routing::{get, post};
use http::Method;
use modo::testing::TestApp;

async fn hello() -> &'static str {
    "hello"
}

#[derive(serde::Deserialize)]
struct EchoBody {
    key: String,
}

impl modo::sanitize::Sanitize for EchoBody {
    fn sanitize(&mut self) {}
}

async fn echo_json(
    modo::extractor::JsonRequest(body): modo::extractor::JsonRequest<EchoBody>,
) -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({"key": body.key}))
}

#[derive(serde::Deserialize)]
struct FormBody {
    name: String,
}

impl modo::sanitize::Sanitize for FormBody {
    fn sanitize(&mut self) {}
}

async fn echo_form(
    modo::extractor::FormRequest(body): modo::extractor::FormRequest<FormBody>,
) -> axum::Json<std::collections::HashMap<String, String>> {
    let mut map = std::collections::HashMap::new();
    map.insert("name".to_string(), body.name);
    axum::Json(map)
}

async fn read_header(headers: http::HeaderMap) -> String {
    headers
        .get("x-custom")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("missing")
        .to_string()
}

async fn greet_user(modo::service::Service(user): modo::service::Service<String>) -> String {
    format!("hello {}", *user)
}

#[tokio::test]
async fn test_get_request() {
    let app = TestApp::builder().route("/", get(hello)).build();

    let res = app.get("/").send().await;
    assert_eq!(res.status(), 200);
    assert_eq!(res.text(), "hello");
}

#[tokio::test]
async fn test_post_json() {
    let app = TestApp::builder().route("/echo", post(echo_json)).build();

    let res = app
        .post("/echo")
        .json(&serde_json::json!({"key": "value"}))
        .send()
        .await;
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json();
    assert_eq!(body["key"], "value");
}

#[tokio::test]
async fn test_post_form() {
    let app = TestApp::builder().route("/form", post(echo_form)).build();

    let mut form = std::collections::HashMap::new();
    form.insert("name", "Alice");

    let res = app.post("/form").form(&form).send().await;
    assert_eq!(res.status(), 200);
    let body: std::collections::HashMap<String, String> = res.json();
    assert_eq!(body["name"], "Alice");
}

#[tokio::test]
async fn test_custom_header() {
    let app = TestApp::builder()
        .route("/header", get(read_header))
        .build();

    let res = app
        .get("/header")
        .header("x-custom", "test-value")
        .send()
        .await;
    assert_eq!(res.status(), 200);
    assert_eq!(res.text(), "test-value");
}

#[tokio::test]
async fn test_service_registration() {
    let app = TestApp::builder()
        .service("world".to_string())
        .route("/greet", get(greet_user))
        .build();

    let res = app.get("/greet").send().await;
    assert_eq!(res.status(), 200);
    assert_eq!(res.text(), "hello world");
}

#[tokio::test]
async fn test_multiple_requests() {
    let app = TestApp::builder().route("/", get(hello)).build();

    let res1 = app.get("/").send().await;
    let res2 = app.get("/").send().await;
    assert_eq!(res1.text(), "hello");
    assert_eq!(res2.text(), "hello");
}

// Handlers must be module-level (closures inside #[tokio::test] don't satisfy Handler bounds)
async fn method_echo(method: http::Method) -> String {
    method.to_string()
}

async fn options_handler() -> &'static str {
    "options"
}

async fn head_handler() -> &'static str {
    ""
}

async fn echo_body(body: axum::body::Bytes) -> axum::body::Bytes {
    body
}

#[tokio::test]
async fn test_put_patch_delete() {
    let app = TestApp::builder()
        .route(
            "/method",
            axum::routing::put(method_echo)
                .patch(method_echo)
                .delete(method_echo),
        )
        .build();

    assert_eq!(app.put("/method").send().await.text(), "PUT");
    assert_eq!(app.patch("/method").send().await.text(), "PATCH");
    assert_eq!(app.delete("/method").send().await.text(), "DELETE");
}

#[tokio::test]
async fn test_options_request() {
    let app = TestApp::builder()
        .route(
            "/opts",
            axum::routing::on(axum::routing::MethodFilter::OPTIONS, options_handler),
        )
        .build();

    let res = app.options("/opts").send().await;
    assert_eq!(res.text(), "options");
}

#[tokio::test]
async fn test_generic_request_method() {
    let app = TestApp::builder()
        .route(
            "/head",
            axum::routing::on(axum::routing::MethodFilter::HEAD, head_handler),
        )
        .build();

    let res = app.request(Method::HEAD, "/head").send().await;
    assert_eq!(res.status(), 200);
}

#[tokio::test]
async fn test_merge_router() {
    let sub = axum::Router::new().route("/sub", get(hello));
    let app = TestApp::builder().merge(sub).build();

    let res = app.get("/sub").send().await;
    assert_eq!(res.status(), 200);
    assert_eq!(res.text(), "hello");
}

#[tokio::test]
async fn test_from_router() {
    let router = axum::Router::new().route("/", get(hello));
    let app = TestApp::from_router(router);

    let res = app.get("/").send().await;
    assert_eq!(res.status(), 200);
    assert_eq!(res.text(), "hello");
}

#[tokio::test]
async fn test_not_found() {
    let app = TestApp::builder().route("/exists", get(hello)).build();

    let res = app.get("/nope").send().await;
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn test_raw_body() {
    let app = TestApp::builder().route("/echo", post(echo_body)).build();

    let res = app.post("/echo").body(b"raw bytes".to_vec()).send().await;
    assert_eq!(res.bytes(), b"raw bytes");
}

#[tokio::test]
async fn test_layer_applies_middleware() {
    let app = TestApp::builder()
        .route("/", get(hello))
        .layer(modo::middleware::request_id())
        .build();

    let res = app.get("/").send().await;
    assert_eq!(res.status(), 200);
    let rid = res
        .header("x-request-id")
        .expect("x-request-id header missing");
    assert_eq!(rid.len(), 26, "expected ULID (26 chars), got: {rid}");
}

#[tokio::test]
async fn test_json_overrides_explicit_content_type() {
    let app = TestApp::builder().route("/echo", post(echo_json)).build();

    let res = app
        .post("/echo")
        .header("content-type", "text/plain")
        .json(&serde_json::json!({"key": "overridden"}))
        .send()
        .await;
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json();
    assert_eq!(body["key"], "overridden");
}
