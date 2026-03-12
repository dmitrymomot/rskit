#![cfg(feature = "sse")]

use futures_util::StreamExt;
use modo::sse::SseBroadcastManager;

// Verify SseStream is publicly exported.
#[allow(unused_imports)]
use modo::sse::SseStream;

#[tokio::test]
async fn broadcast_subscribe_and_send() {
    let mgr: SseBroadcastManager<String, String> = SseBroadcastManager::new(16);
    let mut stream = mgr.subscribe(&"room1".into());

    let count = mgr.send(&"room1".into(), "hello".into()).unwrap();
    assert_eq!(count, 1);

    let item = stream.next().await.unwrap().unwrap();
    assert_eq!(item, "hello");
}

#[tokio::test]
async fn broadcast_multiple_subscribers() {
    let mgr: SseBroadcastManager<String, String> = SseBroadcastManager::new(16);
    let mut s1 = mgr.subscribe(&"room".into());
    let mut s2 = mgr.subscribe(&"room".into());

    let count = mgr.send(&"room".into(), "msg".into()).unwrap();
    assert_eq!(count, 2);

    assert_eq!(s1.next().await.unwrap().unwrap(), "msg");
    assert_eq!(s2.next().await.unwrap().unwrap(), "msg");
}

#[tokio::test]
async fn broadcast_send_to_nonexistent_key_is_noop() {
    let mgr: SseBroadcastManager<String, String> = SseBroadcastManager::new(16);
    let count = mgr.send(&"nobody".into(), "hello".into()).unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn broadcast_subscriber_count() {
    let mgr: SseBroadcastManager<String, i32> = SseBroadcastManager::new(16);
    assert_eq!(mgr.subscriber_count(&"k".into()), 0);

    let _s1 = mgr.subscribe(&"k".into());
    assert_eq!(mgr.subscriber_count(&"k".into()), 1);

    let _s2 = mgr.subscribe(&"k".into());
    assert_eq!(mgr.subscriber_count(&"k".into()), 2);

    drop(_s1);
    // receiver_count decrements immediately on drop; channel stays because _s2 is alive
    assert_eq!(mgr.subscriber_count(&"k".into()), 1);
}

#[tokio::test]
async fn broadcast_auto_cleanup_on_last_unsubscribe() {
    let mgr: SseBroadcastManager<String, String> = SseBroadcastManager::new(16);
    let s = mgr.subscribe(&"temp".into());
    assert_eq!(mgr.subscriber_count(&"temp".into()), 1);

    drop(s);

    // Drop cleanup closure removes the channel immediately — no send() needed
    assert_eq!(mgr.subscriber_count(&"temp".into()), 0);
}

#[tokio::test]
async fn broadcast_remove() {
    let mgr: SseBroadcastManager<String, String> = SseBroadcastManager::new(16);
    let _s = mgr.subscribe(&"room".into());
    assert_eq!(mgr.subscriber_count(&"room".into()), 1);

    mgr.remove(&"room".into());
    assert_eq!(mgr.subscriber_count(&"room".into()), 0);
}

#[tokio::test]
async fn broadcast_stream_closed_when_sender_dropped() {
    let mgr: SseBroadcastManager<String, String> = SseBroadcastManager::new(16);
    let mut stream = mgr.subscribe(&"room".into());

    mgr.remove(&"room".into());

    // Stream should end (return None)
    let next = stream.next().await;
    assert!(next.is_none());
}

#[tokio::test]
async fn broadcast_lagging_subscriber_skips_and_continues() {
    let mgr: SseBroadcastManager<String, String> = SseBroadcastManager::new(2);
    let mut stream = mgr.subscribe(&"room".into());

    // Send 4 messages without reading — buffer is 2, so subscriber will lag
    for i in 0..4 {
        let _ = mgr.send(&"room".into(), format!("msg-{i}"));
    }

    // Stream should still yield values (lagged messages are skipped, not errored)
    let item = stream.next().await;
    assert!(item.is_some(), "stream should still yield after lagging");
    assert!(item.unwrap().is_ok(), "lagged stream item should be Ok");
}

#[tokio::test]
async fn broadcast_cleanup_on_stream_drop() {
    let mgr: SseBroadcastManager<String, String> = SseBroadcastManager::new(16);
    let s = mgr.subscribe(&"room".into());
    assert_eq!(mgr.subscriber_count(&"room".into()), 1);

    drop(s);

    // Channel should be removed immediately by the drop cleanup — no send() needed
    assert_eq!(mgr.subscriber_count(&"room".into()), 0);
}

#[tokio::test]
async fn broadcast_drop_with_remaining_subscribers_keeps_channel() {
    let mgr: SseBroadcastManager<String, String> = SseBroadcastManager::new(16);
    let s1 = mgr.subscribe(&"room".into());
    let _s2 = mgr.subscribe(&"room".into());
    assert_eq!(mgr.subscriber_count(&"room".into()), 2);

    drop(s1);

    // One subscriber remains — channel must stay
    assert_eq!(mgr.subscriber_count(&"room".into()), 1);
}

#[tokio::test]
async fn broadcast_cleanup_targets_only_dropped_channel() {
    let mgr: SseBroadcastManager<String, String> = SseBroadcastManager::new(16);
    let s_a = mgr.subscribe(&"a".into());
    let _s_b = mgr.subscribe(&"b".into());

    assert_eq!(mgr.subscriber_count(&"a".into()), 1);
    assert_eq!(mgr.subscriber_count(&"b".into()), 1);

    drop(s_a);

    // Only "a" should be cleaned up, "b" untouched
    assert_eq!(mgr.subscriber_count(&"a".into()), 0);
    assert_eq!(mgr.subscriber_count(&"b".into()), 1);
}
