#![cfg(feature = "templates")]

use axum::{Router, body::Body, routing::get};
use http::{Request, StatusCode};
use modo::service::Registry;
use modo::template::{Engine, Renderer, TemplateConfig, TemplateContextLayer, context};
use tower::ServiceExt;

// Handlers must be module-level async fn per CLAUDE.md gotcha
async fn home_handler(render: Renderer) -> modo::Result<axum::response::Html<String>> {
    render.html("home.html", context! { name => "World" })
}

async fn partial_handler(render: Renderer) -> modo::Result<axum::response::Html<String>> {
    render.html_partial("home.html", "partial.html", context! { name => "World" })
}

async fn i18n_handler(render: Renderer) -> modo::Result<axum::response::Html<String>> {
    render.html("i18n.html", context! { name => "Dmytro" })
}

fn setup(dir: &std::path::Path) -> Router {
    // Create template files
    let tpl_dir = dir.join("templates");
    std::fs::create_dir_all(&tpl_dir).unwrap();
    std::fs::write(tpl_dir.join("home.html"), "Hello, {{ name }}!").unwrap();
    std::fs::write(tpl_dir.join("partial.html"), "<span>{{ name }}</span>").unwrap();
    std::fs::write(
        tpl_dir.join("i18n.html"),
        "{{ t('common.greeting') }}, {{ name }}!",
    )
    .unwrap();

    // Create locale files
    let en_dir = dir.join("locales/en");
    let uk_dir = dir.join("locales/uk");
    std::fs::create_dir_all(&en_dir).unwrap();
    std::fs::create_dir_all(&uk_dir).unwrap();
    std::fs::write(en_dir.join("common.yaml"), "greeting: Hello").unwrap();
    std::fs::write(uk_dir.join("common.yaml"), "greeting: Привіт").unwrap();

    // Create static files
    let static_dir = dir.join("static/css");
    std::fs::create_dir_all(&static_dir).unwrap();
    std::fs::write(static_dir.join("app.css"), "body { color: red; }").unwrap();

    // Build engine
    let config = TemplateConfig {
        templates_path: tpl_dir.to_str().unwrap().into(),
        locales_path: dir.join("locales").to_str().unwrap().into(),
        static_path: dir.join("static").to_str().unwrap().into(),
        ..TemplateConfig::default()
    };
    let engine = Engine::builder().config(config).build().unwrap();

    // Build router — Engine is Clone (wraps Arc internally), no double-Arc needed
    let mut registry = Registry::new();
    registry.add(engine.clone());

    Router::new()
        .route("/", get(home_handler))
        .route("/partial", get(partial_handler))
        .route("/i18n", get(i18n_handler))
        .layer(TemplateContextLayer::new(engine))
        .with_state(registry.into_state())
}

#[tokio::test]
async fn renders_template() {
    let dir = tempfile::tempdir().unwrap();
    let app = setup(dir.path());

    let resp = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(body, "Hello, World!");
}

#[tokio::test]
async fn html_partial_returns_full_page_for_normal_request() {
    let dir = tempfile::tempdir().unwrap();
    let app = setup(dir.path());

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/partial")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(body, "Hello, World!");
}

#[tokio::test]
async fn html_partial_returns_fragment_for_htmx_request() {
    let dir = tempfile::tempdir().unwrap();
    let app = setup(dir.path());

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/partial")
                .header("hx-request", "true")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(body, "<span>World</span>");
}

#[tokio::test]
async fn i18n_renders_with_default_locale() {
    let dir = tempfile::tempdir().unwrap();
    let app = setup(dir.path());

    let resp = app
        .oneshot(Request::builder().uri("/i18n").body(Body::empty()).unwrap())
        .await
        .unwrap();

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(body, "Hello, Dmytro!");
}

#[tokio::test]
async fn i18n_resolves_locale_from_query_param() {
    let dir = tempfile::tempdir().unwrap();
    let app = setup(dir.path());

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/i18n?lang=uk")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(body, "Привіт, Dmytro!");
}
