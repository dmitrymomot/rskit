#![cfg(feature = "templates")]

use axum::Router;
use axum::body::Body;
use axum::routing::get;
use http::Request;
use modo::templates::middleware::ContextLayer;
use modo::templates::render::RenderLayer;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use tower::ServiceExt;

fn setup(name: &str) -> (Arc<modo::templates::TemplateEngine>, PathBuf) {
    let dir = std::env::temp_dir().join(format!("modo_e2e_test_{name}"));
    let _ = fs::remove_dir_all(&dir);

    let layouts = dir.join("layouts");
    let pages = dir.join("pages");
    let htmx = dir.join("htmx");
    fs::create_dir_all(&layouts).unwrap();
    fs::create_dir_all(&pages).unwrap();
    fs::create_dir_all(&htmx).unwrap();

    fs::write(
        layouts.join("base.html"),
        "<html><body>{% block content %}{% endblock %}</body></html>",
    )
    .unwrap();

    fs::write(
        pages.join("home.html"),
        r#"{% extends "layouts/base.html" %}{% block content %}<h1>{{ title }}</h1><p>url={{ current_url|safe }}</p>{% endblock %}"#,
    )
    .unwrap();

    fs::write(htmx.join("home.html"), "<h1>{{ title }}</h1>").unwrap();

    let config = modo::templates::TemplateConfig {
        path: dir.to_str().unwrap().to_string(),
        ..Default::default()
    };
    let engine = modo::templates::engine(&config).unwrap();
    (Arc::new(engine), dir)
}

#[modo::view("pages/home.html", htmx = "htmx/home.html")]
struct HomePage {
    title: String,
}

#[tokio::test]
async fn full_page_renders_with_layout() {
    let (engine, dir) = setup("fullpage");

    let app = Router::new()
        .route(
            "/",
            get(|| async {
                HomePage {
                    title: "Welcome".to_string(),
                }
            }),
        )
        .layer(RenderLayer::new(engine))
        .layer(ContextLayer::new());

    let resp = app
        .oneshot(Request::get("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let body = String::from_utf8(body.to_vec()).unwrap();
    assert!(body.contains("<html>"));
    assert!(body.contains("<h1>Welcome</h1>"));
    assert!(body.contains("url=/"));

    fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn htmx_request_renders_partial() {
    let (engine, dir) = setup("htmxpartial");

    let app = Router::new()
        .route(
            "/",
            get(|| async {
                HomePage {
                    title: "Welcome".to_string(),
                }
            }),
        )
        .layer(RenderLayer::new(engine))
        .layer(ContextLayer::new());

    let resp = app
        .oneshot(
            Request::get("/")
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
    // Should NOT contain layout
    assert!(!body.contains("<html>"));
    // Should contain partial content
    assert_eq!(body, "<h1>Welcome</h1>");

    fs::remove_dir_all(&dir).unwrap();
}
