use crate::error::Error;
use futures_util::Stream;
use std::collections::HashMap;
use std::hash::Hash;
use std::pin::Pin;
use std::sync::{Arc, RwLock};
use std::task::{Context, Poll};
use tokio::sync::broadcast;

/// Registry of keyed broadcast channels for fan-out SSE delivery.
///
/// Each key maps to an independent broadcast channel. All subscribers of a key
/// receive every message sent to that key. Use one manager per domain concept
/// (e.g., chat messages, uptime checks, notifications).
///
/// # Construction
///
/// ```rust
/// use modo::sse::SseBroadcastManager;
///
/// let chat: SseBroadcastManager<String, String> = SseBroadcastManager::new(128);
/// ```
///
/// Register as a service -- the service registry handles `Arc` wrapping:
/// ```rust,ignore
/// app.service(chat);
/// ```
///
/// # Channel lifecycle
///
/// - Channels are created lazily on first [`subscribe()`](Self::subscribe)
/// - Channels are auto-cleaned when the last subscriber drops (detected on
///   next [`send()`](Self::send) or [`subscribe()`](Self::subscribe) call)
/// - [`remove()`](Self::remove) forces immediate cleanup
///
/// # Filtering
///
/// All subscribers receive all messages. Filter on the consumer side:
/// ```rust,ignore
/// let stream = mgr.subscribe(&room_id)
///     .filter_map(|result| match result {
///         Ok(msg) if msg.sender != my_id => Some(msg),
///         _ => None,
///     });
/// ```
pub struct SseBroadcastManager<K, T>
where
    K: Hash + Eq + Clone + Send + Sync + 'static,
    T: Clone + Send + Sync + 'static,
{
    channels: Arc<RwLock<HashMap<K, broadcast::Sender<T>>>>,
    buffer: usize,
}

impl<K, T> SseBroadcastManager<K, T>
where
    K: Hash + Eq + Clone + Send + Sync + 'static,
    T: Clone + Send + Sync + 'static,
{
    /// Create a new manager with the given per-channel buffer size.
    ///
    /// The buffer size determines how many messages can be buffered before
    /// slow subscribers start lagging (skipping missed messages with a warning).
    /// Typical values: 64-256 for chat, 16-64 for dashboards.
    pub fn new(buffer: usize) -> Self {
        Self {
            channels: Arc::new(RwLock::new(HashMap::new())),
            buffer,
        }
    }

    /// Subscribe to a keyed channel.
    ///
    /// Creates the channel lazily on first subscription. Returns a stream of
    /// raw `T` values -- convert to [`SseEvent`](super::SseEvent) downstream
    /// using [`SseStreamExt`](super::SseStreamExt) or `.map()`.
    pub fn subscribe(&self, key: &K) -> SseStream<T> {
        let mut channels = self.channels.write().unwrap_or_else(|e| e.into_inner());

        // Prune dead channels while we have the lock
        channels.retain(|_, sender| sender.receiver_count() > 0);

        let sender = channels
            .entry(key.clone())
            .or_insert_with(|| broadcast::channel(self.buffer).0);
        let rx = sender.subscribe();
        SseStream::new(rx)
    }

    /// Send an event to all subscribers of a keyed channel.
    ///
    /// Returns the number of receivers that got the message. Returns `Ok(0)`
    /// if no subscribers exist for the key.
    ///
    /// **Does NOT create a channel** -- only [`subscribe()`](Self::subscribe)
    /// creates channels lazily. Sending to a nonexistent key is a silent no-op.
    pub fn send(&self, key: &K, event: T) -> Result<usize, Error> {
        let mut channels = self.channels.write().unwrap_or_else(|e| e.into_inner());

        // Prune dead channels
        channels.retain(|_, sender| sender.receiver_count() > 0);

        if let Some(sender) = channels.get(key) {
            match sender.send(event) {
                Ok(count) => Ok(count),
                Err(_) => Ok(0), // All receivers dropped between retain and send
            }
        } else {
            Ok(0)
        }
    }

    /// Number of active subscribers for a key.
    ///
    /// Returns 0 if the key has no channel.
    pub fn subscriber_count(&self, key: &K) -> usize {
        let channels = self.channels.read().unwrap_or_else(|e| e.into_inner());
        channels.get(key).map(|s| s.receiver_count()).unwrap_or(0)
    }

    /// Manually remove a channel and disconnect all its subscribers.
    ///
    /// Typically not needed -- channels auto-clean when the last subscriber
    /// drops. Use this for explicit teardown (e.g., deleting a chat room).
    pub fn remove(&self, key: &K) {
        let mut channels = self.channels.write().unwrap_or_else(|e| e.into_inner());
        channels.remove(key);
    }
}

// Clone so it can be shared via Arc in the service registry
impl<K, T> Clone for SseBroadcastManager<K, T>
where
    K: Hash + Eq + Clone + Send + Sync + 'static,
    T: Clone + Send + Sync + 'static,
{
    fn clone(&self) -> Self {
        Self {
            channels: Arc::clone(&self.channels),
            buffer: self.buffer,
        }
    }
}

/// A stream of events from a broadcast channel.
///
/// Yields raw `T` values (not [`SseEvent`](super::SseEvent)). Convert
/// downstream using [`SseStreamExt`](super::SseStreamExt) combinators or
/// standard stream methods (`.map()`, `.filter_map()`, etc.).
///
/// # Lagging
///
/// If a subscriber falls behind (buffer full), missed messages are skipped
/// with a warning log. The stream continues with the next available message.
///
/// # End of stream
///
/// The stream ends (`None`) when the broadcast sender is dropped -- either
/// via [`SseBroadcastManager::remove()`] or when the manager itself is dropped.
pub struct SseStream<T> {
    inner: Pin<Box<dyn Stream<Item = Result<T, Error>> + Send>>,
}

impl<T: Clone + Send + 'static> SseStream<T> {
    pub(crate) fn new(rx: broadcast::Receiver<T>) -> Self {
        let stream = futures_util::stream::unfold(rx, |mut rx| async move {
            loop {
                match rx.recv().await {
                    Ok(item) => return Some((Ok(item), rx)),
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(skipped = n, "SSE subscriber lagged, skipping {n} messages");
                        continue;
                    }
                    Err(broadcast::error::RecvError::Closed) => return None,
                }
            }
        });
        Self {
            inner: Box::pin(stream),
        }
    }
}

impl<T> Stream for SseStream<T> {
    type Item = Result<T, Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.inner.as_mut().poll_next(cx)
    }
}
