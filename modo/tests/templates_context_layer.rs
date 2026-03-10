#![cfg(feature = "templates")]

use axum::body::Body;
use axum::routing::get;
use axum::{Extension, Router};
use http::Request;
use modo::templates::TemplateContext;
use modo::templates::middleware::ContextLayer;
use tower::ServiceExt;

async fn handler(Extension(ctx): Extension<TemplateContext>) -> String {
    format!(
        "url={}",
        ctx.get("current_url")
            .map(|v| v.to_string())
            .unwrap_or_default(),
    )
}

#[tokio::test]
async fn context_layer_sets_current_url() {
    let app = Router::new()
        .route("/hello", get(handler))
        .layer(ContextLayer::new());

    let resp = app
        .oneshot(Request::get("/hello").body(Body::empty()).unwrap())
        .await
        .unwrap();

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let body = String::from_utf8(body.to_vec()).unwrap();
    assert!(body.contains("url=/hello"));
}
