use super::event::SseEvent;
use crate::error::Error;
use axum::response::{IntoResponse, Response};
use futures_util::{Stream, StreamExt};
use std::{pin::Pin, time::Duration};

const DEFAULT_KEEP_ALIVE_SECS: u64 = 15;

/// SSE response type for handlers.
///
/// Wraps an event stream with automatic keep-alive. Return this from any
/// handler to start an SSE connection.
///
/// # Construction
///
/// Use [`from_stream()`](super::from_stream) or [`channel()`](super::channel)
/// to create an `SseResponse`. Do not construct directly.
///
/// # Keep-alive
///
/// By default, sends a comment line (`:`) every 15 seconds to keep the
/// connection alive through proxies. Override with
/// [`with_keep_alive()`](Self::with_keep_alive).
///
/// # Example
///
/// ```rust,ignore
/// use modo::sse::{SseEvent, SseResponse};
///
/// #[modo::handler(GET, "/events")]
/// async fn events() -> SseResponse {
///     let stream = futures_util::stream::repeat_with(|| {
///         Ok(SseEvent::new().data("ping"))
///     });
///     modo::sse::from_stream(stream)
/// }
/// ```
pub struct SseResponse {
    stream: Pin<Box<dyn Stream<Item = Result<axum::response::sse::Event, axum::Error>> + Send>>,
    keep_alive_interval: Duration,
}

impl SseResponse {
    pub(crate) fn new<S>(stream: S) -> Self
    where
        S: Stream<Item = Result<axum::response::sse::Event, axum::Error>> + Send + 'static,
    {
        Self {
            stream: Box::pin(stream),
            keep_alive_interval: Duration::from_secs(DEFAULT_KEEP_ALIVE_SECS),
        }
    }

    /// Override the keep-alive interval for this response.
    ///
    /// The default is 15 seconds. Set a shorter interval if your clients are
    /// behind aggressive proxies, or longer to reduce overhead.
    pub fn with_keep_alive(mut self, interval: Duration) -> Self {
        self.keep_alive_interval = interval;
        self
    }
}

impl IntoResponse for SseResponse {
    fn into_response(self) -> Response {
        let mut resp = axum::response::sse::Sse::new(self.stream)
            .keep_alive(axum::response::sse::KeepAlive::new().interval(self.keep_alive_interval))
            .into_response();
        resp.headers_mut()
            .insert(
                "X-Accel-Buffering",
                axum::http::HeaderValue::from_static("no"),
            );
        resp
    }
}

/// Create an [`SseResponse`] from any stream of [`SseEvent`]s.
///
/// This is the main entry point for SSE handlers. The stream is wrapped with
/// automatic keep-alive (configurable via [`SseResponse::with_keep_alive()`]).
///
/// # Type requirements
///
/// The stream must yield `Result<SseEvent, E>` where `E: Into<Error>`.
/// Use [`SseStreamExt`](super::SseStreamExt) combinators or `.map()` to
/// convert domain types to `SseEvent` before passing to this function.
///
/// # Examples
///
/// ```rust,ignore
/// use modo::sse::{SseEvent, SseResponse};
///
/// #[modo::handler(GET, "/events")]
/// async fn events() -> SseResponse {
///     let stream = futures_util::stream::repeat_with(|| {
///         Ok(SseEvent::new().data("ping"))
///     });
///     modo::sse::from_stream(stream)
/// }
/// ```
pub fn from_stream<S, E>(stream: S) -> SseResponse
where
    S: Stream<Item = Result<SseEvent, E>> + Send + 'static,
    E: Into<Error> + Send + 'static,
{
    let mapped = stream.map(|result| {
        result
            .map_err(|e| {
                let err: Error = e.into();
                axum::Error::new(err)
            })
            .and_then(|event| axum::response::sse::Event::try_from(event).map_err(axum::Error::new))
    });
    SseResponse::new(mapped)
}
