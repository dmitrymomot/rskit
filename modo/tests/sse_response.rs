#![cfg(feature = "sse")]

use axum::response::IntoResponse;
use futures_util::stream;
use modo::sse::SseEvent;

#[tokio::test]
async fn sse_response_has_correct_content_type() {
    let s = stream::empty::<Result<SseEvent, modo::Error>>();
    let resp = modo::sse::from_stream(s).into_response();
    let ct = resp
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(ct.contains("text/event-stream"), "got: {ct}");
}

#[tokio::test]
async fn sse_response_has_cache_control() {
    let s = stream::empty::<Result<SseEvent, modo::Error>>();
    let resp = modo::sse::from_stream(s).into_response();
    let cc = resp
        .headers()
        .get("cache-control")
        .unwrap()
        .to_str()
        .unwrap();
    assert_eq!(cc, "no-cache");
}

#[tokio::test]
async fn sse_response_with_keep_alive_override() {
    use std::time::Duration;
    let s = stream::empty::<Result<SseEvent, modo::Error>>();
    // Should not panic
    let resp = modo::sse::from_stream(s)
        .with_keep_alive(Duration::from_secs(60))
        .into_response();
    assert_eq!(resp.status(), http::StatusCode::OK);
}

#[tokio::test]
async fn sse_response_has_x_accel_buffering_header() {
    let s = stream::empty::<Result<SseEvent, modo::Error>>();
    let resp = modo::sse::from_stream(s).into_response();
    let val = resp
        .headers()
        .get("x-accel-buffering")
        .unwrap()
        .to_str()
        .unwrap();
    assert_eq!(val, "no");
}
