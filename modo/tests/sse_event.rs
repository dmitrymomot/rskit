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
