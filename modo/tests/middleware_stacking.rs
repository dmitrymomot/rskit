use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Extension, Router};
use std::sync::{Arc, Mutex};
use tower::ServiceExt;

#[derive(Clone, Default)]
struct ExecutionLog(Arc<Mutex<Vec<String>>>);

impl ExecutionLog {
    fn push(&self, label: &str) {
        self.0.lock().unwrap().push(label.to_string());
    }

    fn entries(&self) -> Vec<String> {
        self.0.lock().unwrap().clone()
    }
}

async fn handler(Extension(log): Extension<ExecutionLog>) -> impl IntoResponse {
    log.push("handler");
    "ok"
}

async fn global_middleware(
    Extension(log): Extension<ExecutionLog>,
    request: Request<Body>,
    next: Next,
) -> Response {
    log.push("global_mw_before");
    let response = next.run(request).await;
    log.push("global_mw_after");
    response
}

async fn module_middleware(
    Extension(log): Extension<ExecutionLog>,
    request: Request<Body>,
    next: Next,
) -> Response {
    log.push("module_mw_before");
    let response = next.run(request).await;
    log.push("module_mw_after");
    response
}

async fn handler_middleware(
    Extension(log): Extension<ExecutionLog>,
    request: Request<Body>,
    next: Next,
) -> Response {
    log.push("handler_mw_before");
    let response = next.run(request).await;
    log.push("handler_mw_after");
    response
}

#[tokio::test]
async fn middleware_executes_in_correct_order() {
    let log = ExecutionLog::default();

    let app = Router::new()
        .route("/test", get(handler))
        .layer(axum::middleware::from_fn(handler_middleware)) // innermost
        .layer(axum::middleware::from_fn(module_middleware)) // middle
        .layer(axum::middleware::from_fn(global_middleware)) // outermost
        .layer(Extension(log.clone()));

    let response = app
        .oneshot(Request::builder().uri("/test").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(&body[..], b"ok");

    let entries = log.entries();
    assert_eq!(
        entries,
        vec![
            "global_mw_before",
            "module_mw_before",
            "handler_mw_before",
            "handler",
            "handler_mw_after",
            "module_mw_after",
            "global_mw_after",
        ]
    );
}

#[tokio::test]
async fn nested_module_middleware_order() {
    let log = ExecutionLog::default();

    let module_router = Router::new()
        .route("/endpoint", get(handler))
        .layer(axum::middleware::from_fn(module_middleware));

    let app = Router::new()
        .nest("/api", module_router)
        .layer(axum::middleware::from_fn(global_middleware))
        .layer(Extension(log.clone()));

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/endpoint")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(&body[..], b"ok");

    let entries = log.entries();
    assert_eq!(
        entries,
        vec![
            "global_mw_before",
            "module_mw_before",
            "handler",
            "module_mw_after",
            "global_mw_after",
        ]
    );
}
