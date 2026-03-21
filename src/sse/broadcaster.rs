use crate::error::Error;
use futures_util::Stream;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::sync::broadcast;

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
    #[allow(dead_code)]
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
#[allow(dead_code)]
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
}
