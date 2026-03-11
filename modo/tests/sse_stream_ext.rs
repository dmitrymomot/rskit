#![cfg(feature = "sse")]

use futures_util::{StreamExt, stream};
use modo::sse::{SseEvent, SseStreamExt};

#[tokio::test]
async fn sse_json_converts_serializable_items() {
    #[derive(Clone, serde::Serialize)]
    struct Msg {
        text: String,
    }

    let items = vec![
        Ok::<_, modo::Error>(Msg {
            text: "hello".into(),
        }),
        Ok(Msg {
            text: "world".into(),
        }),
    ];
    let s = stream::iter(items);
    let events: Vec<_> = s.sse_json().collect().await;
    assert_eq!(events.len(), 2);
    assert!(events[0].is_ok());
    assert!(events[1].is_ok());
}

#[tokio::test]
async fn sse_map_transforms_items() {
    let items = vec![Ok::<_, modo::Error>(42i32), Ok(99)];
    let s = stream::iter(items);
    let events: Vec<_> = s
        .sse_map(|n| Ok(SseEvent::new().event("number").data(n.to_string())))
        .collect()
        .await;
    assert_eq!(events.len(), 2);
    assert!(events[0].is_ok());
}

#[tokio::test]
async fn sse_map_propagates_stream_errors() {
    let items = vec![
        Ok::<_, modo::Error>(1i32),
        Err(modo::Error::internal("fail")),
        Ok(3),
    ];
    let s = stream::iter(items);
    let events: Vec<_> = s
        .sse_map(|n| Ok(SseEvent::new().data(n.to_string())))
        .collect()
        .await;
    assert_eq!(events.len(), 3);
    assert!(events[0].is_ok());
    assert!(events[1].is_err());
    assert!(events[2].is_ok());
}

#[tokio::test]
async fn sse_event_sets_name_and_converts() {
    let items = vec![Ok::<_, modo::Error>(SseEvent::new().data("hello"))];
    let s = stream::iter(items);
    let events: Vec<_> = s.sse_event("ping").collect().await;
    assert_eq!(events.len(), 1);
    assert!(events[0].is_ok());
}
