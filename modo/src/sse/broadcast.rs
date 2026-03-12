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
/// - Channels are auto-cleaned when the last subscriber's [`SseStream`] is
///   dropped — each stream carries a cleanup closure that removes the channel
///   entry if `receiver_count() == 0`
/// - [`send()`](Self::send) also removes a channel on send failure (O(1) targeted)
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
    ///
    /// The returned [`SseStream`] carries a cleanup closure that removes the
    /// channel entry when the last subscriber is dropped — no O(n) scan needed.
    pub fn subscribe(&self, key: &K) -> SseStream<T> {
        let mut channels = self.channels.write().unwrap_or_else(|e| e.into_inner());

        let sender = channels
            .entry(key.clone())
            .or_insert_with(|| broadcast::channel(self.buffer).0);
        let rx = sender.subscribe();

        let channels_ref = Arc::clone(&self.channels);
        let key_owned = key.clone();
        let cleanup = move || {
            // Read-lock fast path: if receivers remain, nothing to do
            {
                let channels = channels_ref.read().unwrap_or_else(|e| e.into_inner());
                if channels
                    .get(&key_owned)
                    .is_none_or(|s| s.receiver_count() > 0)
                {
                    return;
                }
            }
            // Write-lock: double-check before removing (a new subscriber may have joined)
            let mut channels = channels_ref.write().unwrap_or_else(|e| e.into_inner());
            if let std::collections::hash_map::Entry::Occupied(entry) =
                channels.entry(key_owned.clone())
            {
                if entry.get().receiver_count() == 0 {
                    entry.remove();
                }
            }
        };

        SseStream::with_cleanup(rx, cleanup)
    }

    /// Send an event to all subscribers of a keyed channel.
    ///
    /// Returns the number of receivers that got the message. Returns `Ok(0)`
    /// if no subscribers exist for the key.
    ///
    /// **Does NOT create a channel** -- only [`subscribe()`](Self::subscribe)
    /// creates channels lazily. Sending to a nonexistent key is a silent no-op.
    pub fn send(&self, key: &K, event: T) -> Result<usize, Error> {
        // Read lock for the happy path — avoids serializing concurrent senders
        let channels = self.channels.read().unwrap_or_else(|e| e.into_inner());
        if let Some(sender) = channels.get(key) {
            match sender.send(event) {
                Ok(count) => Ok(count),
                Err(_) => {
                    // All receivers dropped — targeted removal (O(1))
                    drop(channels);
                    let mut channels = self.channels.write().unwrap_or_else(|e| e.into_inner());
                    if let std::collections::hash_map::Entry::Occupied(entry) =
                        channels.entry(key.clone())
                    {
                        if entry.get().receiver_count() == 0 {
                            entry.remove();
                        }
                    }
                    Ok(0)
                }
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
///
/// # Cleanup
///
/// When created via [`SseBroadcastManager::subscribe()`], the stream carries a
/// cleanup closure that fires on drop. If this was the last subscriber, the
/// closure removes the channel entry from the manager — no O(n) scan needed.
pub struct SseStream<T> {
    // IMPORTANT: `inner` must be declared before `_cleanup`. Rust drops fields
    // in declaration order — the broadcast `Receiver` inside `inner` must drop
    // first (decrementing `receiver_count`) before the cleanup closure checks it.
    inner: Pin<Box<dyn Stream<Item = Result<T, Error>> + Send>>,
    _cleanup: Option<Box<dyn FnOnce() + Send>>,
}

impl<T: Clone + Send + 'static> SseStream<T> {
    pub(crate) fn with_cleanup(rx: broadcast::Receiver<T>, cleanup: impl FnOnce() + Send + 'static) -> Self {
        Self {
            inner: Box::pin(Self::unfold_stream(rx)),
            _cleanup: Some(Box::new(cleanup)),
        }
    }

    fn unfold_stream(
        rx: broadcast::Receiver<T>,
    ) -> impl Stream<Item = Result<T, Error>> {
        futures_util::stream::unfold(rx, |mut rx| async move {
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
        })
    }
}

impl<T> Drop for SseStream<T> {
    fn drop(&mut self) {
        if let Some(cleanup) = self._cleanup.take() {
            cleanup();
        }
    }
}

impl<T> Stream for SseStream<T> {
    type Item = Result<T, Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.inner.as_mut().poll_next(cx)
    }
}
