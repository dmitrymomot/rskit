use super::event::Event;
use crate::error::Error;
use futures_util::{Stream, StreamExt};

/// Extension trait for converting streams into SSE event streams.
///
/// Operates on streams of `Result<T, E>` — errors pass through unchanged,
/// and the conversion closure only operates on `Ok` values.
///
/// # Usage
///
/// Import the trait and call `cast_events` on any compatible stream:
///
/// ```rust,ignore
/// use modo::sse::{Event, SseStreamExt};
///
/// let stream = bc.subscribe(&key).cast_events(|item| {
///     Event::new(modo::id::short(), "update")?.json(&item)
/// });
/// ```
pub trait SseStreamExt<T, E>: Stream<Item = Result<T, E>> + Sized
where
    E: Into<Error>,
{
    /// Map each item to an [`Event`] with a custom closure.
    ///
    /// Errors from the source stream pass through converted via `Into<Error>`.
    /// Errors returned by the closure also propagate.
    fn cast_events<F>(self, mut f: F) -> impl Stream<Item = Result<Event, Error>> + Send
    where
        F: FnMut(T) -> Result<Event, Error> + Send,
        T: Send,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::Error;
    use futures_util::StreamExt;

    #[tokio::test]
    async fn cast_events_maps_items() {
        let stream = futures_util::stream::iter(vec![
            Ok::<_, Error>("hello".to_string()),
            Ok("world".to_string()),
        ]);

        let events: Vec<super::super::Event> = stream
            .cast_events(|s| super::super::Event::new("id", "msg").map(|e| e.data(s)))
            .filter_map(|r| async { r.ok() })
            .collect()
            .await;

        assert_eq!(events.len(), 2);
        assert_eq!(events[0].data.as_deref(), Some("hello"));
        assert_eq!(events[1].data.as_deref(), Some("world"));
    }

    #[tokio::test]
    async fn cast_events_propagates_source_errors() {
        let stream = futures_util::stream::iter(vec![
            Ok::<_, Error>("ok".to_string()),
            Err(Error::internal("boom")),
        ]);

        let results: Vec<Result<super::super::Event, Error>> = stream
            .cast_events(|s| super::super::Event::new("id", "msg").map(|e| e.data(s)))
            .collect()
            .await;

        assert!(results[0].is_ok());
        assert!(results[1].is_err());
        assert_eq!(results[1].as_ref().unwrap_err().message(), "boom");
    }

    #[tokio::test]
    async fn cast_events_propagates_closure_errors() {
        let stream = futures_util::stream::iter(vec![Ok::<_, Error>("ok".to_string())]);

        let results: Vec<Result<super::super::Event, Error>> = stream
            .cast_events(|_| Err(Error::bad_request("bad")))
            .collect()
            .await;

        assert!(results[0].is_err());
        assert_eq!(results[0].as_ref().unwrap_err().message(), "bad");
    }
}
