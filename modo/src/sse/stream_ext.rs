use super::event::SseEvent;
use crate::error::Error;
use futures_util::{Stream, StreamExt};
use serde::Serialize;

/// Extension trait for converting streams into SSE event streams.
///
/// Operates on streams of `Result<T, E>` -- errors pass through unchanged,
/// and the conversion closure/trait only operates on `Ok` values.
///
/// # Usage
///
/// Import the trait and call methods on any compatible stream:
///
/// ```rust,ignore
/// use modo::sse::SseStreamExt;
///
/// // JSON conversion
/// let stream = bc.subscribe(&key).sse_json();
///
/// // Custom mapping
/// let stream = bc.subscribe(&key).sse_map(|item| {
///     Ok(SseEvent::new().event("update").json(&item)?)
/// });
/// ```
pub trait SseStreamExt<T, E>: Stream<Item = Result<T, E>> + Sized
where
    E: Into<Error>,
{
    /// Set event name on each item and pass through.
    ///
    /// Items must already be [`SseEvent`]s (or convertible via `Into<SseEvent>`).
    /// This is useful when you have a stream of pre-built events and want to
    /// override or set the event name uniformly.
    fn with_event_name(
        self,
        name: &'static str,
    ) -> impl Stream<Item = Result<SseEvent, Error>> + Send
    where
        T: Into<SseEvent> + Send,
        E: Send,
        Self: Send,
    {
        self.map(move |result| {
            result
                .map_err(Into::into)
                .map(|item| item.into().event(name))
        })
    }

    /// Serialize each item as JSON data in an [`SseEvent`].
    ///
    /// Equivalent to `.sse_map(|item| SseEvent::new().json(&item))`.
    ///
    /// # Errors
    ///
    /// Yields an error if JSON serialization fails for any item.
    fn sse_json(self) -> impl Stream<Item = Result<SseEvent, Error>> + Send
    where
        T: Serialize + Send,
        E: Send,
        Self: Send,
    {
        self.map(|result| {
            result
                .map_err(Into::into)
                .and_then(|item| SseEvent::new().json(&item))
        })
    }

    /// Map each item to an [`SseEvent`] with a custom closure.
    ///
    /// The closure receives the unwrapped `Ok` value. Stream errors pass
    /// through unchanged.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let stream = bc.subscribe(&room_id).sse_map(|msg| {
    ///     let html = view.render_to_string(MessageView::from(&msg))?;
    ///     Ok(SseEvent::new().event("message").html(html))
    /// });
    /// ```
    fn sse_map<F>(self, mut f: F) -> impl Stream<Item = Result<SseEvent, Error>> + Send
    where
        F: FnMut(T) -> Result<SseEvent, Error> + Send,
        E: Send,
        Self: Send,
    {
        self.map(move |result| result.map_err(Into::into).and_then(&mut f))
    }
}

// Blanket implementation for all compatible streams
impl<S, T, E> SseStreamExt<T, E> for S
where
    S: Stream<Item = Result<T, E>> + Sized,
    E: Into<Error>,
{
}
