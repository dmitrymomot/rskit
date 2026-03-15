#![cfg(feature = "templates")]

use axum::body::Body;
use axum::http::Request;
use axum::middleware::{self, Next};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Extension, Router};
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

async fn pre_ctx_middleware(mut request: Request<Body>, next: Next) -> impl IntoResponse {
    let mut ctx = TemplateContext::new();
    ctx.insert("pre_value", "hello");
    request.extensions_mut().insert(ctx);
    next.run(request).await
}

async fn merge_handler(Extension(ctx): Extension<TemplateContext>) -> String {
    let pre = ctx
        .get("pre_value")
        .map(|v| v.to_string())
        .unwrap_or_default();
    let url = ctx
        .get("current_url")
        .map(|v| v.to_string())
        .unwrap_or_default();
    format!("pre={pre},url={url}")
}

#[tokio::test]
async fn context_layer_merges_with_existing_context() {
    let app = Router::new()
        .route("/test", get(merge_handler))
        .layer(ContextLayer::new())
        .layer(middleware::from_fn(pre_ctx_middleware));

    let resp = app
        .oneshot(Request::get("/test").body(Body::empty()).unwrap())
        .await
        .unwrap();

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let body = String::from_utf8(body.to_vec()).unwrap();
    assert!(
        body.contains("pre=hello"),
        "pre_value should survive: {body}"
    );
    assert!(
        body.contains("url=/test"),
        "current_url should be set: {body}"
    );
}
