#![cfg(feature = "sse")]

use futures_util::StreamExt;
use modo::sse::{Broadcaster, Event, LagPolicy, SseConfig, SseStreamExt, replay};

#[tokio::test]
async fn full_broadcast_flow() {
    let bc: Broadcaster<String, String> = Broadcaster::new(64, SseConfig::default());
    let key = "room".to_string();

    // Subscribe
    let stream = bc.subscribe(&key).on_lag(LagPolicy::End);

    // Send
    bc.send(&key, "hello".into());
    bc.send(&key, "world".into());

    // Collect first two items
    let items: Vec<String> = stream
        .take(2)
        .filter_map(|r| async { r.ok() })
        .collect()
        .await;

    assert_eq!(items, vec!["hello", "world"]);
}

#[tokio::test]
async fn broadcast_with_cast_events() {
    let bc: Broadcaster<String, i32> = Broadcaster::new(64, SseConfig::default());
    let key = "metrics".to_string();

    let stream = bc
        .subscribe(&key)
        .on_lag(LagPolicy::Skip)
        .cast_events(|val| Event::new("id", "metric")?.json(&val));

    bc.send(&key, 42);

    let events: Vec<Event> = stream
        .take(1)
        .filter_map(|r| async { r.ok() })
        .collect()
        .await;

    assert_eq!(events.len(), 1);
    assert_eq!(events[0].data_ref(), Some("42"));
}

#[tokio::test]
async fn replay_then_live() {
    let bc: Broadcaster<String, String> = Broadcaster::new(64, SseConfig::default());
    let key = "chat".to_string();

    let live = bc.subscribe(&key).on_lag(LagPolicy::End);
    let missed = vec!["replay1".to_string(), "replay2".to_string()];

    // Send a live message
    bc.send(&key, "live1".into());

    let stream = replay(missed).chain(live);

    let items: Vec<String> = stream
        .take(3)
        .filter_map(|r| async { r.ok() })
        .collect()
        .await;

    assert_eq!(items, vec!["replay1", "replay2", "live1"]);
}

#[tokio::test]
async fn response_has_correct_headers() {
    let bc: Broadcaster<String, String> = Broadcaster::new(16, SseConfig::default());
    let stream = futures_util::stream::empty::<Result<Event, modo::Error>>();
    let response = bc.response(stream);

    assert_eq!(response.headers().get("x-accel-buffering").unwrap(), "no");
    assert_eq!(
        response.headers().get("content-type").unwrap(),
        "text/event-stream"
    );
}
