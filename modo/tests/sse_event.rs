#![cfg(feature = "sse")]

use modo::sse::SseEvent;
use std::time::Duration;

#[test]
fn event_with_data() {
    let event = SseEvent::new().data("hello");
    let _axum_event: axum::response::sse::Event = event.try_into().unwrap();
    // Event has data set — we verify it doesn't panic during conversion
}

#[test]
fn event_with_json() {
    #[derive(serde::Serialize)]
    struct Msg {
        text: String,
    }
    let event = SseEvent::new()
        .event("message")
        .json(&Msg {
            text: "hello".into(),
        })
        .unwrap();
    let _axum_event: axum::response::sse::Event = event.try_into().unwrap();
}

#[test]
fn event_with_html() {
    let event = SseEvent::new().event("update").html("<div>hello</div>");
    let _axum_event: axum::response::sse::Event = event.try_into().unwrap();
}

#[test]
fn event_with_id_and_retry() {
    let event = SseEvent::new()
        .data("hello")
        .id("evt-1")
        .retry(Duration::from_secs(5));
    let _axum_event: axum::response::sse::Event = event.try_into().unwrap();
}

#[test]
fn event_json_overrides_data() {
    #[derive(serde::Serialize)]
    struct Msg {
        n: i32,
    }
    // json() after data() should override
    let event = SseEvent::new().data("old").json(&Msg { n: 42 }).unwrap();
    let _: axum::response::sse::Event = event.try_into().unwrap();
}

#[test]
fn event_json_serialization_error() {
    // Custom type that always fails to serialize
    struct AlwaysFail;
    impl serde::Serialize for AlwaysFail {
        fn serialize<S: serde::Serializer>(&self, _s: S) -> Result<S::Ok, S::Error> {
            Err(serde::ser::Error::custom("intentional failure"))
        }
    }
    let result = SseEvent::new().json(&AlwaysFail);
    assert!(result.is_err());
}

#[test]
fn event_default_has_no_data() {
    let event = SseEvent::new();
    // Converting event without data should still work (axum allows it)
    let _axum_event: axum::response::sse::Event = event.try_into().unwrap();
}

// --- Wire format tests ---

#[tokio::test]
async fn event_wire_format_data_only() {
    use axum::response::IntoResponse;
    use futures_util::stream;

    let s = stream::iter(vec![Ok::<_, modo::Error>(SseEvent::new().data("hello"))]);
    let response = modo::sse::from_stream(s).into_response();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let text = String::from_utf8(body.to_vec()).unwrap();
    assert!(
        text.contains("data: hello"),
        "expected 'data: hello' in: {text}"
    );
}

#[tokio::test]
async fn event_wire_format_with_event_name_and_id() {
    use axum::response::IntoResponse;
    use futures_util::stream;

    let s = stream::iter(vec![Ok::<_, modo::Error>(
        SseEvent::new().event("update").data("payload").id("evt-1"),
    )]);
    let response = modo::sse::from_stream(s).into_response();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let text = String::from_utf8(body.to_vec()).unwrap();
    assert!(
        text.contains("event: update"),
        "expected 'event: update' in: {text}"
    );
    assert!(
        text.contains("data: payload"),
        "expected 'data: payload' in: {text}"
    );
    assert!(
        text.contains("id: evt-1"),
        "expected 'id: evt-1' in: {text}"
    );
}

#[tokio::test]
async fn event_wire_format_multiline_data() {
    use axum::response::IntoResponse;
    use futures_util::stream;

    let s = stream::iter(vec![Ok::<_, modo::Error>(
        SseEvent::new().data("line1\nline2\nline3"),
    )]);
    let response = modo::sse::from_stream(s).into_response();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let text = String::from_utf8(body.to_vec()).unwrap();
    assert!(
        text.contains("data: line1"),
        "expected 'data: line1' in: {text}"
    );
    assert!(
        text.contains("data: line2"),
        "expected 'data: line2' in: {text}"
    );
    assert!(
        text.contains("data: line3"),
        "expected 'data: line3' in: {text}"
    );
}

#[tokio::test]
async fn event_wire_format_with_retry() {
    use axum::response::IntoResponse;
    use futures_util::stream;

    let s = stream::iter(vec![Ok::<_, modo::Error>(
        SseEvent::new().data("hi").retry(Duration::from_secs(5)),
    )]);
    let response = modo::sse::from_stream(s).into_response();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let text = String::from_utf8(body.to_vec()).unwrap();
    assert!(
        text.contains("retry: 5000"),
        "expected 'retry: 5000' in: {text}"
    );
}
