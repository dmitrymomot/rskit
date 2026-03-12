#![cfg(feature = "sse")]

use axum::{Router, response::IntoResponse, routing::get};
use http::{Request, StatusCode};
use modo::sse::LastEventId;
use tower::ServiceExt;

async fn handler(last_id: LastEventId) -> impl IntoResponse {
    match last_id.0 {
        Some(id) => format!("reconnect:{id}"),
        None => "first-connect".to_string(),
    }
}

fn app() -> Router {
    Router::new().route("/events", get(handler))
}

#[tokio::test]
async fn last_event_id_absent() {
    let resp = app()
        .oneshot(
            Request::get("/events")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(&body[..], b"first-connect");
}

#[tokio::test]
async fn last_event_id_present() {
    let resp = app()
        .oneshot(
            Request::get("/events")
                .header("Last-Event-ID", "evt-42")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(&body[..], b"reconnect:evt-42");
}

#[tokio::test]
async fn last_event_id_none_for_non_utf8_header() {
    let resp = app()
        .oneshot(
            Request::get("/events")
                .header("Last-Event-ID", b"\xff\xfe".as_slice())
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(&body[..], b"first-connect");
}
