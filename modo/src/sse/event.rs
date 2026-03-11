use crate::error::Error;
use serde::Serialize;
use std::time::Duration;

/// A Server-Sent Event to be delivered to a connected client.
///
/// Uses a builder pattern to construct events with optional fields.
/// At minimum, an event should have data (via [`data()`](Self::data),
/// [`json()`](Self::json), or [`html()`](Self::html)).
///
/// # Examples
///
/// ```rust
/// use modo::sse::SseEvent;
///
/// // Plain text event
/// let event = SseEvent::new()
///     .event("message")
///     .data("Hello, world!");
///
/// // JSON event
/// # use serde::Serialize;
/// # #[derive(Serialize)]
/// # struct Status { ok: bool }
/// let event = SseEvent::new()
///     .event("status")
///     .json(&Status { ok: true })
///     .unwrap();
///
/// // HTML partial (for HTMX)
/// let event = SseEvent::new()
///     .event("update")
///     .html("<div class=\"card\">New content</div>");
/// ```
///
/// # Data methods
///
/// [`data()`](Self::data), [`json()`](Self::json), and [`html()`](Self::html) are
/// mutually exclusive — each sets the event's data payload, replacing any
/// previous value. [`json()`](Self::json) is fallible because serialization can fail.
///
/// # SSE protocol fields
///
/// | Method | SSE field | Purpose |
/// |--------|-----------|---------|
/// | [`event()`](Self::event) | `event:` | Named event type for client-side listeners |
/// | [`id()`](Self::id) | `id:` | Last-event-id for reconnection (see [`LastEventId`](super::LastEventId)) |
/// | [`retry()`](Self::retry) | `retry:` | Client reconnect delay hint (serialized as milliseconds) |
#[must_use]
#[derive(Debug, Clone)]
pub struct SseEvent {
    pub(crate) event_name: Option<String>,
    pub(crate) data: Option<String>,
    pub(crate) id: Option<String>,
    pub(crate) retry: Option<Duration>,
}

impl SseEvent {
    /// Create a new empty SSE event.
    pub fn new() -> Self {
        Self {
            event_name: None,
            data: None,
            id: None,
            retry: None,
        }
    }

    /// Set the event type name.
    ///
    /// Clients can listen for specific event types. In JavaScript:
    /// `eventSource.addEventListener("message", handler)`.
    /// In HTMX: `hx-trigger="sse:message"`.
    pub fn event(mut self, name: impl Into<String>) -> Self {
        self.event_name = Some(name.into());
        self
    }

    /// Set the data payload as a plain string.
    ///
    /// Multi-line strings are handled automatically — each line gets its own
    /// `data:` prefix per the SSE spec. The browser reassembles them with `\n`.
    pub fn data(mut self, data: impl Into<String>) -> Self {
        self.data = Some(data.into());
        self
    }

    /// Set the data payload as JSON-serialized data.
    ///
    /// Mutually exclusive with [`data()`](Self::data) and [`html()`](Self::html)
    /// — replaces any previously set data.
    ///
    /// # Errors
    ///
    /// Returns an error if `serde_json` serialization fails (e.g., the value
    /// contains `f64::NAN`).
    pub fn json<T: Serialize>(mut self, data: &T) -> Result<Self, Error> {
        let json = serde_json::to_string(data)
            .map_err(|e| Error::internal(format!("SSE JSON serialization failed: {e}")))?;
        self.data = Some(json);
        Ok(self)
    }

    /// Set the data payload as an HTML fragment.
    ///
    /// Semantically identical to [`data()`](Self::data) — the SSE protocol
    /// treats all data as text. This method communicates intent and may gain
    /// additional behavior (e.g., escaping) in the future.
    ///
    /// **Best practice:** Keep HTML partials small — send individual components
    /// (a chat bubble, a table row, a notification card), not entire page sections.
    /// SSE has no built-in chunking or compression.
    pub fn html(self, html: impl Into<String>) -> Self {
        self.data(html)
    }

    /// Set the event ID for client reconnection.
    ///
    /// When a client reconnects, the browser sends a `Last-Event-ID` header
    /// with this value. Use [`LastEventId`](super::LastEventId) to read it
    /// in the handler and replay missed events.
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// Set the reconnection delay hint for the client.
    ///
    /// Tells the browser how long to wait before reconnecting after a
    /// disconnect. Serialized as milliseconds in the SSE `retry:` field.
    pub fn retry(mut self, duration: Duration) -> Self {
        self.retry = Some(duration);
        self
    }
}

impl Default for SseEvent {
    fn default() -> Self {
        Self::new()
    }
}

impl TryFrom<SseEvent> for axum::response::sse::Event {
    type Error = Error;

    fn try_from(sse: SseEvent) -> Result<Self, Self::Error> {
        let mut event = axum::response::sse::Event::default();
        if let Some(name) = sse.event_name {
            event = event.event(name);
        }
        if let Some(data) = sse.data {
            event = event.data(data);
        }
        if let Some(id) = sse.id {
            event = event.id(id);
        }
        if let Some(retry) = sse.retry {
            event = event.retry(retry);
        }
        Ok(event)
    }
}
