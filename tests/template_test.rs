use axum::{Router, body::Body, routing::get};
use http::{Request, StatusCode};
use modo::service::Registry;
use modo::template::{Engine, Renderer, TemplateConfig, TemplateContextLayer, context};
use tempfile::TempDir;
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

async fn string_handler(render: Renderer) -> Result<String, modo::Error> {
    render.string("home.html", context! { name => "World" })
}

async fn missing_handler(render: Renderer) -> Result<axum::response::Html<String>, modo::Error> {
    render.html("nonexistent.html", context! {})
}

async fn static_url_handler(render: Renderer) -> Result<axum::response::Html<String>, modo::Error> {
    render.html("assets.html", context! {})
}

fn setup() -> (TempDir, Router) {
    let dir = tempfile::tempdir().unwrap();

    // Create template files
    let tpl_dir = dir.path().join("templates");
    std::fs::create_dir_all(&tpl_dir).unwrap();
    std::fs::write(tpl_dir.join("home.html"), "Hello, {{ name }}!").unwrap();
    std::fs::write(tpl_dir.join("partial.html"), "<span>{{ name }}</span>").unwrap();
    std::fs::write(
        tpl_dir.join("i18n.html"),
        "{{ t('common.greeting') }}, {{ name }}!",
    )
    .unwrap();
    std::fs::write(
        tpl_dir.join("assets.html"),
        "{{ static_url('css/app.css') }}",
    )
    .unwrap();

    // Create locale files
    let en_dir = dir.path().join("locales/en");
    let uk_dir = dir.path().join("locales/uk");
    std::fs::create_dir_all(&en_dir).unwrap();
    std::fs::create_dir_all(&uk_dir).unwrap();
    std::fs::write(en_dir.join("common.yaml"), "greeting: Hello").unwrap();
    std::fs::write(uk_dir.join("common.yaml"), "greeting: Привіт").unwrap();

    // Create static files
    let static_dir = dir.path().join("static/css");
    std::fs::create_dir_all(&static_dir).unwrap();
    std::fs::write(static_dir.join("app.css"), "body { color: red; }").unwrap();

    // Build engine
    let config = {
        let mut c = TemplateConfig::default();
        c.templates_path = tpl_dir.to_str().unwrap().into();
        c.locales_path = dir.path().join("locales").to_str().unwrap().into();
        c.static_path = dir.path().join("static").to_str().unwrap().into();
        c
    };
    let engine = Engine::builder().config(config).build().unwrap();

    // Build router — Engine is Clone (wraps Arc internally), no double-Arc needed
    let mut registry = Registry::new();
    registry.add(engine.clone());

    let router = Router::new()
        .route("/", get(home_handler))
        .route("/partial", get(partial_handler))
        .route("/i18n", get(i18n_handler))
        .route("/string", get(string_handler))
        .route("/missing", get(missing_handler))
        .route("/assets", get(static_url_handler))
        .layer(TemplateContextLayer::new(engine))
        .with_state(registry.into_state());

    (dir, router)
}

#[tokio::test]
async fn renders_template() {
    let (_dir, app) = setup();

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
    let (_dir, app) = setup();

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
    let (_dir, app) = setup();

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
    let (_dir, app) = setup();

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
    let (_dir, app) = setup();

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

#[tokio::test]
async fn i18n_resolves_locale_from_cookie() {
    let (_dir, app) = setup();
    let req = Request::builder()
        .uri("/i18n?name=Dmytro")
        .header("cookie", "lang=uk")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_str = std::str::from_utf8(&body).unwrap();
    assert!(body_str.contains("Привіт"));
}

#[tokio::test]
async fn i18n_resolves_locale_from_accept_language() {
    let (_dir, app) = setup();
    let req = Request::builder()
        .uri("/i18n?name=Dmytro")
        .header("accept-language", "uk;q=0.9, en;q=0.8")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_str = std::str::from_utf8(&body).unwrap();
    assert!(body_str.contains("Привіт"));
}

#[tokio::test]
async fn render_string_from_handler() {
    let (_dir, app) = setup();
    let req = Request::builder()
        .uri("/string")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(body, "Hello, World!");
}

#[tokio::test]
async fn missing_template_returns_500() {
    let (_dir, app) = setup();
    let req = Request::builder()
        .uri("/missing")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn static_url_in_template() {
    let (_dir, app) = setup();
    let req = Request::builder()
        .uri("/assets")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_str = std::str::from_utf8(&body).unwrap();
    assert!(body_str.starts_with("/assets/css/app.css?v="));
    // Hash is 8 hex chars
    assert_eq!(body_str.len(), "/assets/css/app.css?v=".len() + 8);
}
