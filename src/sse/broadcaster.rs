use crate::error::Error;
use axum::response::{IntoResponse, Response};
use futures_util::{FutureExt, Stream, StreamExt};
use std::collections::HashMap;
use std::hash::Hash;
use std::pin::Pin;
use std::sync::{Arc, RwLock};
use std::task::{Context, Poll};
use tokio::sync::broadcast;

use super::config::SseConfig;
use super::event::Event;

/// Policy for handling lagged subscribers in a broadcast stream.
#[derive(Debug, Clone, Copy)]
pub enum LagPolicy {
    /// End the stream on lag — client reconnects with `Last-Event-ID`.
    End,
    /// Skip lagged messages with a warning log, continue streaming.
    Skip,
}

/// A stream of events from a broadcast channel.
///
/// Yields raw `T` values (not [`Event`](super::Event)). Convert downstream
/// using [`SseStreamExt::cast_events()`](super::SseStreamExt::cast_events).
///
/// # Lag behavior
///
/// Configure with [`on_lag()`](Self::on_lag):
/// - [`LagPolicy::End`] — stream terminates (safe for chat/notifications)
/// - [`LagPolicy::Skip`] — skips missed messages (safe for dashboards)
/// - Default (no call) — propagates lag as [`Error`]
pub struct BroadcastStream<T> {
    // IMPORTANT: `inner` must be declared before `_cleanup`. Rust drops fields
    // in declaration order — the broadcast `Receiver` inside `inner` must drop
    // first (decrementing `receiver_count`) before the cleanup closure checks it.
    inner: Pin<Box<dyn Stream<Item = Result<T, Error>> + Send>>,
    _cleanup: Option<Box<dyn FnOnce() + Send>>,
}

impl<T: Clone + Send + 'static> BroadcastStream<T> {
    /// Create a new broadcast stream (no cleanup).
    #[allow(dead_code)]
    pub(crate) fn new(rx: broadcast::Receiver<T>) -> Self {
        Self {
            inner: Box::pin(unfold_default(rx)),
            _cleanup: None,
        }
    }

    /// Create a new broadcast stream with a cleanup closure.
    pub(crate) fn with_cleanup(
        rx: broadcast::Receiver<T>,
        cleanup: impl FnOnce() + Send + 'static,
    ) -> Self {
        Self {
            inner: Box::pin(unfold_default(rx)),
            _cleanup: Some(Box::new(cleanup)),
        }
    }

    /// Set the lag policy for this stream.
    ///
    /// - [`LagPolicy::End`] — end the stream on lag. Client reconnects with
    ///   `Last-Event-ID` and replays from their store. Use for chat,
    ///   notifications, anything where message loss is unacceptable.
    /// - [`LagPolicy::Skip`] — skip lagged messages with a warning log and
    ///   continue. Use for dashboards, metrics, anything where the next
    ///   value supersedes the previous.
    ///
    /// Default (no call): propagate the lag error through the stream as
    /// [`Error`] — caller handles it via standard stream combinators.
    pub fn on_lag(mut self, policy: LagPolicy) -> Self {
        // Reconstruct the inner stream with the new policy.
        // We wrap the existing stream with policy handling.
        let original = std::mem::replace(&mut self.inner, Box::pin(futures_util::stream::empty()));
        self.inner = Box::pin(apply_lag_policy(original, policy));
        self
    }
}

/// Default unfold: propagate lag errors.
fn unfold_default<T: Clone + Send + 'static>(
    rx: broadcast::Receiver<T>,
) -> impl Stream<Item = Result<T, Error>> {
    futures_util::stream::unfold(rx, |mut rx| async move {
        match rx.recv().await {
            Ok(item) => Some((Ok(item), rx)),
            Err(broadcast::error::RecvError::Lagged(n)) => Some((Err(Error::lagged(n)), rx)),
            Err(broadcast::error::RecvError::Closed) => None,
        }
    })
}

/// Wrap a stream with lag policy handling.
fn apply_lag_policy<T: Send + 'static>(
    stream: Pin<Box<dyn Stream<Item = Result<T, Error>> + Send>>,
    policy: LagPolicy,
) -> impl Stream<Item = Result<T, Error>> + Send {
    futures_util::stream::unfold(stream, move |mut stream| async move {
        use futures_util::StreamExt;
        loop {
            match stream.next().await {
                Some(Ok(item)) => return Some((Ok(item), stream)),
                Some(Err(e)) if e.is_lagged() => match policy {
                    LagPolicy::End => return None,
                    LagPolicy::Skip => {
                        tracing::warn!("{e}");
                        continue;
                    }
                },
                Some(Err(e)) => return Some((Err(e), stream)),
                None => return None,
            }
        }
    })
}

impl<T> Drop for BroadcastStream<T> {
    fn drop(&mut self) {
        if let Some(cleanup) = self._cleanup.take() {
            cleanup();
        }
    }
}

impl<T> Stream for BroadcastStream<T> {
    type Item = Result<T, Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.inner.as_mut().poll_next(cx)
    }
}

/// Convert a `Vec<T>` into a `Stream<Item = Result<T, Error>>`.
///
/// Use this to replay missed events from a data store before chaining
/// with a live broadcast stream on client reconnection.
///
/// The returned stream yields each item wrapped in `Ok`. Chain it with
/// a live [`BroadcastStream`] using `.chain()` from
/// [`futures_util::StreamExt`].
pub fn replay<T>(items: Vec<T>) -> impl Stream<Item = Result<T, Error>> + Send
where
    T: Send + 'static,
{
    futures_util::stream::iter(items.into_iter().map(Ok))
}

struct BroadcasterInner<K, T> {
    channels: RwLock<HashMap<K, broadcast::Sender<T>>>,
    buffer: usize,
    config: SseConfig,
}

/// Keyed broadcast channel registry for fan-out SSE delivery.
///
/// Each key maps to an independent broadcast channel. All subscribers of a key
/// receive every message sent to that key. Register one broadcaster per domain
/// concept (e.g., chat messages, notifications, metrics).
///
/// # Construction
///
/// ```
/// use modo::sse::{Broadcaster, SseConfig};
///
/// # #[derive(Clone)]
/// # struct ChatMessage;
/// let chat: Broadcaster<String, ChatMessage> =
///     Broadcaster::new(128, SseConfig::default());
/// # let mut registry = modo::service::Registry::new();
/// registry.add(chat);
/// ```
///
/// # Channel lifecycle
///
/// - Channels are created lazily on first [`subscribe()`](Self::subscribe)
/// - Channels are auto-cleaned when the last subscriber's stream is dropped
/// - [`remove()`](Self::remove) forces immediate cleanup
pub struct Broadcaster<K, T>
where
    K: Hash + Eq + Clone + Send + Sync + 'static,
    T: Clone + Send + Sync + 'static,
{
    inner: Arc<BroadcasterInner<K, T>>,
}

impl<K, T> Clone for Broadcaster<K, T>
where
    K: Hash + Eq + Clone + Send + Sync + 'static,
    T: Clone + Send + Sync + 'static,
{
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<K, T> Broadcaster<K, T>
where
    K: Hash + Eq + Clone + Send + Sync + 'static,
    T: Clone + Send + Sync + 'static,
{
    /// Create a new broadcaster.
    ///
    /// - `buffer` — per-channel buffer size. When a subscriber falls behind
    ///   by this many messages, it lags. Typical values: 64–256 for chat,
    ///   16–64 for dashboards.
    /// - `config` — SSE configuration (keep-alive interval).
    pub fn new(buffer: usize, config: SseConfig) -> Self {
        Self {
            inner: Arc::new(BroadcasterInner {
                channels: RwLock::new(HashMap::new()),
                buffer,
                config,
            }),
        }
    }

    /// Subscribe to a keyed channel.
    ///
    /// Creates the channel lazily on first subscription. Returns a stream
    /// of raw `T` values. The stream carries a cleanup closure that removes
    /// the channel entry when the last subscriber drops.
    pub fn subscribe(&self, key: &K) -> BroadcastStream<T> {
        let mut channels = self
            .inner
            .channels
            .write()
            .unwrap_or_else(|e| e.into_inner());

        let sender = channels
            .entry(key.clone())
            .or_insert_with(|| broadcast::channel(self.inner.buffer).0);
        let rx = sender.subscribe();

        let inner_ref = Arc::clone(&self.inner);
        let key_owned = key.clone();
        let cleanup = move || {
            let mut channels = inner_ref
                .channels
                .write()
                .unwrap_or_else(|e| e.into_inner());
            if let std::collections::hash_map::Entry::Occupied(entry) = channels.entry(key_owned)
                && entry.get().receiver_count() == 0
            {
                entry.remove();
            }
        };

        BroadcastStream::with_cleanup(rx, cleanup)
    }

    /// Send an event to all subscribers of a key.
    ///
    /// Returns the number of receivers that got the message. Returns 0
    /// if no subscribers exist for the key — does NOT create a channel.
    pub fn send(&self, key: &K, event: T) -> usize {
        let channels = self
            .inner
            .channels
            .read()
            .unwrap_or_else(|e| e.into_inner());
        if let Some(sender) = channels.get(key) {
            match sender.send(event) {
                Ok(count) => count,
                Err(_) => {
                    drop(channels);
                    let mut channels = self
                        .inner
                        .channels
                        .write()
                        .unwrap_or_else(|e| e.into_inner());
                    if let std::collections::hash_map::Entry::Occupied(entry) =
                        channels.entry(key.clone())
                        && entry.get().receiver_count() == 0
                    {
                        entry.remove();
                    }
                    0
                }
            }
        } else {
            0
        }
    }

    /// Number of active subscribers for a key. Returns 0 if no channel exists.
    pub fn subscriber_count(&self, key: &K) -> usize {
        let channels = self
            .inner
            .channels
            .read()
            .unwrap_or_else(|e| e.into_inner());
        channels.get(key).map(|s| s.receiver_count()).unwrap_or(0)
    }

    /// Manually remove a channel and disconnect all its subscribers.
    ///
    /// Typically not needed — channels auto-clean on last subscriber drop.
    /// Use for explicit teardown (e.g., deleting a chat room).
    pub fn remove(&self, key: &K) {
        let mut channels = self
            .inner
            .channels
            .write()
            .unwrap_or_else(|e| e.into_inner());
        channels.remove(key);
    }

    /// Access the SSE config.
    pub fn config(&self) -> &SseConfig {
        &self.inner.config
    }

    /// Create an SSE response with an imperative sender.
    ///
    /// Spawns the closure as a tokio task. The closure receives a [`super::Sender`]
    /// for pushing events. The task runs until:
    /// - The closure returns `Ok(())` — stream ends cleanly
    /// - The closure returns `Err(e)` — error is logged, stream ends
    /// - A `tx.send()` call fails — client disconnected
    ///
    /// Panics in the closure are caught and logged.
    pub fn channel<F, Fut>(&self, f: F) -> Response
    where
        F: FnOnce(super::Sender) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = Result<(), Error>> + Send,
    {
        const CHANNEL_BUFFER: usize = 32;
        let (tx, rx) = tokio::sync::mpsc::channel(CHANNEL_BUFFER);
        let sender = super::Sender { tx };

        tokio::spawn(async move {
            let result = std::panic::AssertUnwindSafe(f(sender)).catch_unwind().await;
            match result {
                Ok(Ok(())) => {}
                Ok(Err(e)) => {
                    tracing::debug!(error = %e, "SSE channel closure ended with error")
                }
                Err(_) => tracing::error!("SSE channel closure panicked"),
            }
        });

        // Wrap the mpsc receiver as a stream of Events
        let stream = futures_util::stream::unfold(rx, |mut rx| async move {
            rx.recv().await.map(|event| (Ok(event), rx))
        });

        self.response(stream)
    }

    /// Wrap an event stream into an SSE HTTP response.
    ///
    /// Applies keep-alive comments at the configured interval and sets
    /// the `X-Accel-Buffering: no` header for nginx compatibility.
    pub fn response<S>(&self, stream: S) -> Response
    where
        S: Stream<Item = Result<Event, Error>> + Send + 'static,
    {
        let mapped = stream.map(|result| {
            result
                .map(axum::response::sse::Event::from)
                .map_err(axum::Error::new)
        });

        let keep_alive =
            axum::response::sse::KeepAlive::new().interval(self.inner.config.keep_alive_interval());

        let mut resp = axum::response::sse::Sse::new(mapped)
            .keep_alive(keep_alive)
            .into_response();

        resp.headers_mut()
            .insert("x-accel-buffering", http::HeaderValue::from_static("no"));

        resp
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::StreamExt;
    use tokio::sync::broadcast;

    #[tokio::test]
    async fn stream_yields_sent_values() {
        let (tx, rx) = broadcast::channel(16);
        let mut stream = BroadcastStream::new(rx);
        tx.send("hello".to_string()).unwrap();
        tx.send("world".to_string()).unwrap();
        drop(tx);

        let items: Vec<String> = stream
            .by_ref()
            .filter_map(|r| async { r.ok() })
            .collect()
            .await;
        assert_eq!(items, vec!["hello", "world"]);
    }

    #[tokio::test]
    async fn stream_ends_when_sender_dropped() {
        let (tx, rx) = broadcast::channel(16);
        let mut stream = BroadcastStream::new(rx);
        tx.send(1).unwrap();
        drop(tx);

        assert!(stream.next().await.unwrap().is_ok()); // 1
        assert!(stream.next().await.is_none()); // end
    }

    #[tokio::test]
    async fn lag_policy_skip_continues_after_lag() {
        let (tx, rx) = broadcast::channel(2);
        let mut stream = BroadcastStream::new(rx).on_lag(LagPolicy::Skip);

        // Fill buffer beyond capacity to cause lag
        tx.send(1).unwrap();
        tx.send(2).unwrap();
        tx.send(3).unwrap(); // overwrites 1, receiver lags

        // Should skip lagged messages and yield the latest
        let item = stream.next().await.unwrap();
        assert!(item.is_ok());
    }

    #[tokio::test]
    async fn lag_policy_end_terminates_on_lag() {
        let (tx, rx) = broadcast::channel(2);
        let mut stream = BroadcastStream::new(rx).on_lag(LagPolicy::End);

        tx.send(1).unwrap();
        tx.send(2).unwrap();
        tx.send(3).unwrap(); // causes lag

        let item = stream.next().await;
        assert!(item.is_none()); // stream ended
    }

    #[tokio::test]
    async fn default_lag_policy_propagates_error() {
        let (tx, rx) = broadcast::channel(2);
        let mut stream = BroadcastStream::new(rx);

        tx.send(1).unwrap();
        tx.send(2).unwrap();
        tx.send(3).unwrap(); // causes lag

        let item = stream.next().await.unwrap();
        assert!(item.is_err());
        assert!(item.unwrap_err().is_lagged());
    }

    #[tokio::test]
    async fn replay_yields_all_items() {
        let items = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let stream = replay(items);
        let collected: Vec<String> = stream.filter_map(|r| async { r.ok() }).collect().await;
        assert_eq!(collected, vec!["a", "b", "c"]);
    }

    #[tokio::test]
    async fn replay_empty_vec() {
        let stream = replay::<String>(vec![]);
        let collected: Vec<String> = stream.filter_map(|r| async { r.ok() }).collect().await;
        assert!(collected.is_empty());
    }

    #[tokio::test]
    async fn cleanup_fires_on_drop() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, Ordering};

        let (tx, rx) = broadcast::channel::<i32>(16);
        let cleaned = Arc::new(AtomicBool::new(false));
        let cleaned_clone = cleaned.clone();

        let stream = BroadcastStream::with_cleanup(rx, move || {
            cleaned_clone.store(true, Ordering::SeqCst);
        });

        drop(stream);
        assert!(cleaned.load(Ordering::SeqCst));
        drop(tx);
    }

    #[tokio::test]
    async fn broadcaster_subscribe_and_send() {
        let bc: Broadcaster<String, String> = Broadcaster::new(16, SseConfig::default());
        let key = "room1".to_string();

        let mut stream = bc.subscribe(&key);
        assert_eq!(bc.subscriber_count(&key), 1);

        let count = bc.send(&key, "hello".into());
        assert_eq!(count, 1);

        let item = stream.next().await.unwrap().unwrap();
        assert_eq!(item, "hello");
    }

    #[tokio::test]
    async fn broadcaster_send_to_nonexistent_key_returns_zero() {
        let bc: Broadcaster<String, String> = Broadcaster::new(16, SseConfig::default());
        let count = bc.send(&"nobody".into(), "hello".into());
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn broadcaster_multiple_subscribers() {
        let bc: Broadcaster<String, i32> = Broadcaster::new(16, SseConfig::default());
        let key = "k".to_string();

        let mut s1 = bc.subscribe(&key);
        let mut s2 = bc.subscribe(&key);
        assert_eq!(bc.subscriber_count(&key), 2);

        bc.send(&key, 42);
        assert_eq!(s1.next().await.unwrap().unwrap(), 42);
        assert_eq!(s2.next().await.unwrap().unwrap(), 42);
    }

    #[tokio::test]
    async fn broadcaster_auto_cleanup_on_last_drop() {
        let bc: Broadcaster<String, i32> = Broadcaster::new(16, SseConfig::default());
        let key = "cleanup".to_string();

        let s1 = bc.subscribe(&key);
        let s2 = bc.subscribe(&key);
        assert_eq!(bc.subscriber_count(&key), 2);

        drop(s1);
        // Channel still exists (s2 is alive)
        assert_eq!(bc.subscriber_count(&key), 1);

        drop(s2);
        // Channel should be cleaned up
        assert_eq!(bc.subscriber_count(&key), 0);
    }

    #[tokio::test]
    async fn broadcaster_remove_disconnects_subscribers() {
        let bc: Broadcaster<String, i32> = Broadcaster::new(16, SseConfig::default());
        let key = "rm".to_string();

        let mut stream = bc.subscribe(&key);
        bc.remove(&key);

        // Stream should end because sender was dropped
        assert!(stream.next().await.is_none());
    }

    #[tokio::test]
    async fn broadcaster_clone_shares_state() {
        let bc1: Broadcaster<String, String> = Broadcaster::new(16, SseConfig::default());
        let bc2 = bc1.clone();
        let key = "shared".to_string();

        let mut stream = bc1.subscribe(&key);
        bc2.send(&key, "from_clone".into());

        let item = stream.next().await.unwrap().unwrap();
        assert_eq!(item, "from_clone");
    }

    #[tokio::test]
    async fn broadcaster_channel_produces_events() {
        let bc: Broadcaster<String, String> = Broadcaster::new(16, SseConfig::default());

        let response = bc.channel(|tx| async move {
            tx.send(super::Event::new("e1", "test").unwrap().data("hello"))
                .await?;
            tx.send(super::Event::new("e2", "test").unwrap().data("world"))
                .await?;
            Ok(())
        });

        // Response should have SSE headers
        assert_eq!(response.headers().get("x-accel-buffering").unwrap(), "no");
        assert_eq!(
            response.headers().get("content-type").unwrap(),
            "text/event-stream"
        );
    }

    #[test]
    fn broadcaster_config_accessible() {
        let config = SseConfig {
            keep_alive_interval_secs: 30,
        };
        let bc: Broadcaster<String, String> = Broadcaster::new(64, config);
        assert_eq!(bc.config().keep_alive_interval_secs, 30);
    }

    #[tokio::test]
    async fn broadcaster_response_returns_valid_response() {
        let bc: Broadcaster<String, String> = Broadcaster::new(16, SseConfig::default());
        let stream = futures_util::stream::empty::<Result<super::Event, crate::error::Error>>();
        let response = bc.response(stream);
        assert_eq!(response.headers().get("x-accel-buffering").unwrap(), "no");
        assert_eq!(
            response.headers().get("content-type").unwrap(),
            "text/event-stream"
        );
    }

    #[tokio::test]
    async fn channel_closure_error_produces_valid_response() {
        let bc: Broadcaster<String, String> = Broadcaster::new(16, SseConfig::default());

        let response =
            bc.channel(|_tx| async move { Err(crate::error::Error::internal("deliberate error")) });

        assert_eq!(
            response.headers().get("content-type").unwrap(),
            "text/event-stream"
        );
        assert_eq!(response.headers().get("x-accel-buffering").unwrap(), "no");
    }

    #[tokio::test]
    async fn channel_closure_panic_produces_valid_response() {
        let bc: Broadcaster<String, String> = Broadcaster::new(16, SseConfig::default());

        let response = bc.channel(|_tx| async move {
            panic!("deliberate panic");
        });

        assert_eq!(
            response.headers().get("content-type").unwrap(),
            "text/event-stream"
        );
        assert_eq!(response.headers().get("x-accel-buffering").unwrap(), "no");
    }

    #[tokio::test]
    async fn concurrent_subscribe_and_send() {
        let bc: Broadcaster<String, i32> = Broadcaster::new(256, SseConfig::default());
        let key = "concurrent".to_string();

        let mut set = tokio::task::JoinSet::new();

        for task_num in 0..10 {
            let bc = bc.clone();
            let key = key.clone();
            set.spawn(async move {
                let mut stream = bc.subscribe(&key);
                bc.send(&key, task_num);
                stream.next().await.unwrap().unwrap()
            });
        }

        let mut results = Vec::new();
        while let Some(result) = set.join_next().await {
            results.push(result.expect("Task panicked"));
        }

        assert_eq!(results.len(), 10);
    }
}
