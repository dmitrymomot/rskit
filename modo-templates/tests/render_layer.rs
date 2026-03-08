use axum::body::Body;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use http::Request;
use modo_templates::middleware::ContextLayer;
use modo_templates::render::RenderLayer;
use modo_templates::View;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use tower::ServiceExt;

fn setup(name: &str) -> (Arc<modo_templates::TemplateEngine>, PathBuf) {
    let dir = std::env::temp_dir().join(format!("modo_render_test_{name}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("hello.html"),
        "Hello {{ name }}! url={{ current_url|safe }}",
    )
    .unwrap();
    fs::write(dir.join("hello_htmx.html"), "partial: {{ name }}").unwrap();

    let config = modo_templates::TemplateConfig {
        path: dir.to_str().unwrap().to_string(),
        ..Default::default()
    };
    let engine = modo_templates::engine(&config).unwrap();
    (Arc::new(engine), dir)
}

#[tokio::test]
async fn renders_view_with_merged_context() {
    let (engine, dir) = setup("merged");

    async fn handler() -> impl IntoResponse {
        View::new(
            "hello.html",
            minijinja::context! { name => "World" }.into(),
        )
    }

    let app = Router::new()
        .route("/test", get(handler))
        .layer(RenderLayer::new(engine))
        .layer(ContextLayer::new());

    let resp = app
        .oneshot(Request::get("/test").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let body = String::from_utf8(body.to_vec()).unwrap();
    assert_eq!(body, "Hello World! url=/test");

    fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn htmx_request_uses_htmx_template() {
    let (engine, dir) = setup("htmx");

    async fn handler() -> impl IntoResponse {
        View::new(
            "hello.html",
            minijinja::context! { name => "World" }.into(),
        )
        .with_htmx("hello_htmx.html")
    }

    let app = Router::new()
        .route("/test", get(handler))
        .layer(RenderLayer::new(engine))
        .layer(ContextLayer::new());

    let resp = app
        .oneshot(
            Request::get("/test")
                .header("hx-request", "true")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let body = String::from_utf8(body.to_vec()).unwrap();
    assert_eq!(body, "partial: World");

    fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn non_view_response_passes_through() {
    let (engine, dir) = setup("passthrough");

    async fn handler() -> &'static str {
        "plain text"
    }

    let app = Router::new()
        .route("/test", get(handler))
        .layer(RenderLayer::new(engine))
        .layer(ContextLayer::new());

    let resp = app
        .oneshot(Request::get("/test").body(Body::empty()).unwrap())
        .await
        .unwrap();

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(String::from_utf8(body.to_vec()).unwrap(), "plain text");

    fs::remove_dir_all(&dir).unwrap();
}
