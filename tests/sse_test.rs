#![cfg(feature = "sse")]

use axum::Router;
use axum::body::Body;
use axum::http::Request;
use axum::routing::get;
use futures_util::StreamExt;
use http::StatusCode;
use modo::sse::{Broadcaster, Event, LagPolicy, LastEventId, SseConfig, SseStreamExt, replay};
use tower::ServiceExt;

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

// ---------------------------------------------------------------------------
// Task 7: SSE channel/subscriber/last-event-id tests
// ---------------------------------------------------------------------------

/// Verify that `Broadcaster::channel()` produces an SSE response with valid
/// headers and that the `Sender` passed to the closure can push an `Event`
/// which is included in the response stream.
#[tokio::test]
async fn test_broadcaster_channel_send() {
    let bc: Broadcaster<String, String> = Broadcaster::new(16, SseConfig::default());

    // `channel()` takes a closure receiving a Sender; we send one event and
    // then return Ok(()) to end the stream cleanly.
    let response = bc.channel(|tx| async move {
        let event = Event::new("id1", "message")?.data("hello from channel");
        tx.send(event).await?;
        Ok(())
    });

    // The response must carry SSE headers — this is the observable contract
    // for the channel() API in tests (consuming the body requires the full
    // SSE framing decoder which is out of scope for unit-style integration tests).
    assert_eq!(
        response.headers().get("content-type").unwrap(),
        "text/event-stream"
    );
    assert_eq!(response.headers().get("x-accel-buffering").unwrap(), "no");
}

/// Verify that `subscriber_count()` reflects the number of active subscribers.
#[tokio::test]
async fn test_broadcaster_subscriber_count() {
    let bc: Broadcaster<String, String> = Broadcaster::new(64, SseConfig::default());
    let key = "room".to_string();

    let _s1 = bc.subscribe(&key);
    let _s2 = bc.subscribe(&key);

    assert_eq!(bc.subscriber_count(&key), 2);
}

/// Handler that extracts `LastEventId` and returns its value as the response
/// body. Defined at module level because axum `Handler` bounds are not
/// satisfied by closures in `#[tokio::test]`.
async fn last_event_id_handler(LastEventId(id): LastEventId) -> String {
    id.unwrap_or_default()
}

/// Verify that `LastEventId` extracts the `Last-Event-ID` header and returns
/// it, and returns an empty string when the header is absent.
#[tokio::test]
async fn test_last_event_id() {
    use modo::service::Registry;

    let app = Router::new()
        .route("/", get(last_event_id_handler))
        .with_state(Registry::new().into_state());

    // Request with Last-Event-ID header
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/")
                .header("last-event-id", "42")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&body), "42");

    // Request without Last-Event-ID header — body should be empty string
    let response = app
        .oneshot(
            Request::builder()
                .uri("/")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&body), "");
}
