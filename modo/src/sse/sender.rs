use super::event::SseEvent;
use super::response::SseResponse;
use crate::error::Error;
use futures_util::{FutureExt, Stream};
use std::panic::AssertUnwindSafe;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::sync::mpsc;

const CHANNEL_BUFFER: usize = 32;

/// Sender for imperative SSE event production.
///
/// Used within [`channel()`](super::channel) closures to push events to the
/// connected client. When the client disconnects, [`send()`](Self::send)
/// returns an error.
///
/// # Cleanup
///
/// Cleanup is cooperative: the spawned task runs until the closure returns
/// or a `send()` call fails. Handlers producing messages in a loop should
/// check the `send()` result and break on error.
///
/// # Example
///
/// ```rust,ignore
/// modo::sse::channel(|tx| async move {
///     for i in 0..10 {
///         tx.send(SseEvent::new().data(format!("count: {i}"))).await?;
///         tokio::time::sleep(Duration::from_secs(1)).await;
///     }
///     Ok(())
/// })
/// ```
pub struct SseSender {
    tx: mpsc::Sender<SseEvent>,
}

impl SseSender {
    /// Send an event to the connected client.
    ///
    /// # Errors
    ///
    /// Returns an error if the client has disconnected (the response stream
    /// was dropped). Use this as a signal to stop producing events.
    pub async fn send(&self, event: SseEvent) -> Result<(), Error> {
        self.tx
            .send(event)
            .await
            .map_err(|_| Error::internal("SSE client disconnected"))
    }
}

/// Receiver stream that wraps `mpsc::Receiver<SseEvent>` for use in `SseResponse`.
struct ReceiverStream {
    rx: mpsc::Receiver<SseEvent>,
}

impl Stream for ReceiverStream {
    type Item = Result<SseEvent, Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.rx.poll_recv(cx).map(|opt| opt.map(Ok))
    }
}

/// Create an [`SseResponse`] with an imperative sender.
///
/// Spawns the closure as a tokio task and returns an SSE response backed by
/// the receiver end of an internal channel. The closure receives an
/// [`SseSender`] for pushing events.
///
/// The task runs until:
/// - The closure returns `Ok(())` — stream ends cleanly
/// - The closure returns `Err(e)` — error is logged, stream ends
/// - A `tx.send()` call fails — client disconnected
///
/// # Example
///
/// ```rust,ignore
/// #[modo::handler(GET, "/jobs/{id}/progress")]
/// async fn job_progress(id: String, Service(jobs): Service<JobService>) -> SseResponse {
///     modo::sse::channel(|tx| async move {
///         while let Some(status) = jobs.poll_status(&id).await {
///             tx.send(SseEvent::new().event("progress").json(&status)?).await?;
///             if status.is_done() { break; }
///         }
///         Ok(())
///     })
/// }
/// ```
pub fn channel<F, Fut>(f: F) -> SseResponse
where
    F: FnOnce(SseSender) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = Result<(), Error>> + Send,
{
    let (tx, rx) = mpsc::channel(CHANNEL_BUFFER);
    let sender = SseSender { tx };

    tokio::spawn(async move {
        let result = AssertUnwindSafe(f(sender)).catch_unwind().await;
        match result {
            Ok(Ok(())) => {}
            Ok(Err(e)) => tracing::debug!(error = %e, "SSE channel closure ended with error"),
            Err(_) => tracing::error!("SSE channel closure panicked"),
        }
    });

    let stream = ReceiverStream { rx };
    super::response::from_stream(stream)
}
