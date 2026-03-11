#![cfg(feature = "templates")]

use axum::{
    body::Body,
    extract::Extension,
    routing::{get, post},
    Router,
};
use http::{Request, StatusCode};
use modo::templates::{engine, ContextLayer, TemplateConfig, TemplateEngine, ViewRenderer};
use modo::ViewResult;
use std::io::Write;
use std::sync::Arc;
use tempfile::TempDir;
use tower::ServiceExt;

fn setup(templates: &[(&str, &str)]) -> (TempDir, Arc<TemplateEngine>) {
    let dir = TempDir::new().unwrap();
    for (name, content) in templates {
        let path = dir.path().join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        let mut f = std::fs::File::create(path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
    }
    let config = TemplateConfig {
        path: dir.path().to_string_lossy().to_string(),
        ..Default::default()
    };
    let eng = engine(&config).unwrap();
    (dir, Arc::new(eng))
}

#[modo::view("hello.html")]
struct HelloView {
    name: String,
}

#[modo::view("toast.html")]
struct ToastView {
    message: String,
}

#[modo::view("page.html", htmx = "partial.html")]
struct DualView {
    title: String,
}

// Test handler: single view
async fn single_view(view: ViewRenderer) -> ViewResult {
    view.render(HelloView { name: "World".into() })
}

// Test handler: tuple of views
async fn multi_view(view: ViewRenderer) -> ViewResult {
    view.render((
        HelloView { name: "Alice".into() },
        ToastView {
            message: "Done!".into(),
        },
    ))
}

// Test handler: smart redirect
async fn redirect_handler(view: ViewRenderer) -> ViewResult {
    view.redirect("/dashboard")
}

// Test handler: is_htmx check
async fn check_htmx(view: ViewRenderer) -> String {
    format!("{}", view.is_htmx())
}

// Test handler: dual template
async fn dual_template(view: ViewRenderer) -> ViewResult {
    view.render(DualView {
        title: "Test".into(),
    })
}

// Test handler: render_to_string
async fn render_string(view: ViewRenderer) -> String {
    view.render_to_string(HelloView {
        name: "String".into(),
    })
    .unwrap()
}

fn app(engine: Arc<TemplateEngine>) -> Router {
    Router::new()
        .route("/hello", get(single_view))
        .route("/multi", get(multi_view))
        .route("/redirect", post(redirect_handler))
        .route("/check-htmx", get(check_htmx))
        .route("/dual", get(dual_template))
        .route("/render-string", get(render_string))
        .layer(ContextLayer::new())
        .layer(Extension(engine))
}

async fn body_string(resp: axum::response::Response) -> String {
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    String::from_utf8(bytes.to_vec()).unwrap()
}

#[tokio::test]
async fn render_single_view() {
    let (_dir, eng) = setup(&[("hello.html", "Hello {{ name }}!")]);
    let resp = app(eng)
        .oneshot(Request::get("/hello").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(body_string(resp).await, "Hello World!");
}

#[tokio::test]
async fn render_tuple_of_views() {
    let (_dir, eng) = setup(&[
        ("hello.html", "Hello {{ name }}!"),
        ("toast.html", "<div>{{ message }}</div>"),
    ]);
    let resp = app(eng)
        .oneshot(Request::get("/multi").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(body_string(resp).await, "Hello Alice!<div>Done!</div>");
}

#[tokio::test]
async fn redirect_normal_request() {
    let (_dir, eng) = setup(&[]);
    let resp = app(eng)
        .oneshot(
            Request::post("/redirect")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::FOUND);
    assert_eq!(resp.headers().get("location").unwrap(), "/dashboard");
}

#[tokio::test]
async fn redirect_htmx_request() {
    let (_dir, eng) = setup(&[]);
    let resp = app(eng)
        .oneshot(
            Request::post("/redirect")
                .header("hx-request", "true")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(resp.headers().get("hx-redirect").unwrap(), "/dashboard");
}

#[tokio::test]
async fn is_htmx_detection() {
    let (_dir, eng) = setup(&[]);

    // Normal request
    let resp = app(Arc::clone(&eng))
        .oneshot(Request::get("/check-htmx").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(body_string(resp).await, "false");

    // HTMX request
    let resp = app(Arc::clone(&eng))
        .oneshot(
            Request::get("/check-htmx")
                .header("hx-request", "true")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(body_string(resp).await, "true");
}

#[tokio::test]
async fn dual_template_selects_htmx_partial() {
    let (_dir, eng) = setup(&[
        ("page.html", "Full: {{ title }}"),
        ("partial.html", "Partial: {{ title }}"),
    ]);

    // Normal request — full page
    let resp = app(Arc::clone(&eng))
        .oneshot(Request::get("/dual").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(body_string(resp).await, "Full: Test");

    // HTMX request — partial
    let resp = app(eng)
        .oneshot(
            Request::get("/dual")
                .header("hx-request", "true")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(body_string(resp).await, "Partial: Test");
}

#[tokio::test]
async fn dual_template_adds_vary_header() {
    let (_dir, eng) = setup(&[
        ("page.html", "Full: {{ title }}"),
        ("partial.html", "Partial: {{ title }}"),
    ]);
    let resp = app(eng)
        .oneshot(Request::get("/dual").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(resp.headers().get("vary").unwrap(), "HX-Request");
}

#[tokio::test]
async fn render_to_string_works() {
    let (_dir, eng) = setup(&[("hello.html", "Hello {{ name }}!")]);
    let resp = app(eng)
        .oneshot(
            Request::get("/render-string")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(body_string(resp).await, "Hello String!");
}
