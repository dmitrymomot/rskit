use crate::error::Error;
use serde::Serialize;
use std::time::Duration;

/// A Server-Sent Event to be delivered to a connected client.
///
/// Uses a builder pattern. `id` and `event` name are required at construction
/// and validated — `\n` and `\r` are rejected.
///
/// # Examples
///
/// ```rust,ignore
/// use modo::sse::Event;
///
/// let event = Event::new("evt_01", "message")?.data("Hello, world!");
/// let event = Event::new(id::short(), "status")?.json(&status)?;
/// let event = Event::new(id::short(), "update")?.html("<div>new</div>");
/// ```
#[must_use]
#[derive(Debug, Clone)]
pub struct Event {
    pub(crate) id: String,
    pub(crate) event: String,
    pub(crate) data: Option<String>,
    pub(crate) retry: Option<Duration>,
}

fn validate_field(value: &str, field_name: &str) -> Result<(), Error> {
    if value.contains('\n') || value.contains('\r') {
        return Err(Error::bad_request(format!(
            "SSE {field_name} must not contain newline characters"
        )));
    }
    Ok(())
}

impl Event {
    /// Create a new event. Both `id` and `event` are required.
    ///
    /// - `id` maps to the SSE `id:` field — used by clients for `Last-Event-ID`
    ///   on reconnection.
    /// - `event` maps to the SSE `event:` field — clients listen for specific
    ///   event types (e.g., `eventSource.addEventListener("message", handler)`
    ///   or HTMX `hx-trigger="sse:message"`).
    ///
    /// # Errors
    ///
    /// Returns an error if `id` or `event` contain `\n` or `\r`.
    pub fn new(id: impl Into<String>, event: impl Into<String>) -> Result<Self, Error> {
        let id = id.into();
        let event = event.into();
        validate_field(&id, "id")?;
        validate_field(&event, "event")?;
        Ok(Self {
            id,
            event,
            data: None,
            retry: None,
        })
    }

    /// Set the data payload as a plain string.
    ///
    /// Multi-line strings are handled automatically per the SSE spec — each
    /// line gets its own `data:` prefix. The browser reassembles them with `\n`.
    pub fn data(mut self, data: impl Into<String>) -> Self {
        self.data = Some(data.into());
        self
    }

    /// Set the data payload as JSON-serialized data.
    ///
    /// Replaces any previous data. Returns an error if serialization fails.
    pub fn json<T: Serialize>(mut self, data: &T) -> Result<Self, Error> {
        let json = serde_json::to_string(data)
            .map_err(|e| Error::internal(format!("SSE JSON serialization failed: {e}")))?;
        self.data = Some(json);
        Ok(self)
    }

    /// Set the data payload as an HTML fragment.
    ///
    /// Semantically identical to [`data()`](Self::data). Communicates intent
    /// for HTMX partial rendering use cases.
    pub fn html(self, html: impl Into<String>) -> Self {
        self.data(html)
    }

    /// Set the reconnection delay hint for the client.
    ///
    /// Serialized as milliseconds in the SSE `retry:` field. Tells the browser
    /// how long to wait before reconnecting after a disconnect.
    pub fn retry(mut self, duration: Duration) -> Self {
        self.retry = Some(duration);
        self
    }

    /// Returns the event ID.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Returns the event name.
    pub fn event_name(&self) -> &str {
        &self.event
    }

    /// Returns the data payload, if set.
    pub fn data_ref(&self) -> Option<&str> {
        self.data.as_deref()
    }
}

impl From<Event> for axum::response::sse::Event {
    fn from(event: Event) -> Self {
        let mut axum_event = axum::response::sse::Event::default();
        axum_event = axum_event.id(event.id);
        axum_event = axum_event.event(event.event);
        if let Some(data) = event.data {
            axum_event = axum_event.data(data);
        }
        if let Some(retry) = event.retry {
            axum_event = axum_event.retry(retry);
        }
        axum_event
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_with_valid_id_and_event() {
        let event = Event::new("evt_01", "message").unwrap();
        assert_eq!(event.id, "evt_01");
        assert_eq!(event.event, "message");
        assert!(event.data.is_none());
        assert!(event.retry.is_none());
    }

    #[test]
    fn new_rejects_newline_in_id() {
        let result = Event::new("evt\n01", "message");
        assert!(result.is_err());
        assert!(result.unwrap_err().message().contains("id"));
    }

    #[test]
    fn new_rejects_carriage_return_in_event() {
        let result = Event::new("evt_01", "msg\r");
        assert!(result.is_err());
        assert!(result.unwrap_err().message().contains("event"));
    }

    #[test]
    fn data_sets_payload() {
        let event = Event::new("id", "ev").unwrap().data("hello");
        assert_eq!(event.data.as_deref(), Some("hello"));
    }

    #[test]
    fn json_serializes_payload() {
        #[derive(serde::Serialize)]
        struct Msg {
            text: String,
        }
        let event = Event::new("id", "ev")
            .unwrap()
            .json(&Msg { text: "hi".into() })
            .unwrap();
        assert_eq!(event.data.as_deref(), Some(r#"{"text":"hi"}"#));
    }

    #[test]
    fn html_sets_payload() {
        let event = Event::new("id", "ev").unwrap().html("<div>hi</div>");
        assert_eq!(event.data.as_deref(), Some("<div>hi</div>"));
    }

    #[test]
    fn retry_sets_duration() {
        let event = Event::new("id", "ev")
            .unwrap()
            .retry(std::time::Duration::from_secs(5));
        assert_eq!(event.retry, Some(std::time::Duration::from_secs(5)));
    }

    #[test]
    fn from_converts_to_axum_event() {
        let event = Event::new("id1", "message")
            .unwrap()
            .data("hello")
            .retry(std::time::Duration::from_millis(3000));
        let axum_event: axum::response::sse::Event = event.into();
        let _ = axum_event;
    }

    #[test]
    fn data_methods_replace_previous() {
        let event = Event::new("id", "ev").unwrap().data("first").html("second");
        assert_eq!(event.data.as_deref(), Some("second"));
    }

    #[test]
    fn new_with_empty_id_and_event_succeeds() {
        let event = Event::new("", "").unwrap();
        assert_eq!(event.id, "");
        assert_eq!(event.event, "");
    }

    #[test]
    fn new_rejects_carriage_return_in_id() {
        let result = Event::new("evt\r01", "message");
        assert!(result.is_err());
        assert!(result.unwrap_err().message().contains("id"));
    }

    #[test]
    fn new_rejects_newline_in_event() {
        let result = Event::new("evt_01", "msg\n");
        assert!(result.is_err());
        assert!(result.unwrap_err().message().contains("event"));
    }

    #[test]
    fn getter_methods_return_expected_values() {
        let event = Event::new("id1", "update").unwrap().data("payload");
        assert_eq!(event.id(), "id1");
        assert_eq!(event.event_name(), "update");
        assert_eq!(event.data_ref(), Some("payload"));
    }

    #[test]
    fn data_ref_returns_none_when_no_data() {
        let event = Event::new("id1", "ping").unwrap();
        assert!(event.data_ref().is_none());
    }
}
