use axum::extract::FromRequestParts;
use http::request::Parts;

/// Extracts the `Last-Event-ID` header from the request.
///
/// When a client reconnects after a disconnect, the browser's `EventSource`
/// sends a `Last-Event-ID` header with the ID of the last event it received.
/// Use this extractor to detect reconnections and replay missed events.
///
/// Contains `None` on first connection (header absent).
///
/// # Replay is application logic
///
/// The SSE module does NOT replay events automatically. Your handler is
/// responsible for fetching missed events from your data store.
#[derive(Debug, Clone)]
pub struct LastEventId(pub Option<String>);

impl<S> FromRequestParts<S> for LastEventId
where
    S: Send + Sync,
{
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let value = parts
            .headers
            .get("last-event-id")
            .and_then(|v| v.to_str().ok())
            .map(String::from);
        Ok(LastEventId(value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::FromRequestParts;
    use http::Request;

    #[tokio::test]
    async fn extracts_last_event_id_header() {
        let (mut parts, _body) = Request::builder()
            .header("last-event-id", "evt_42")
            .body(())
            .unwrap()
            .into_parts();
        let result = LastEventId::from_request_parts(&mut parts, &()).await;
        let last_id = result.unwrap();
        assert_eq!(last_id.0, Some("evt_42".to_string()));
    }

    #[tokio::test]
    async fn returns_none_when_header_absent() {
        let (mut parts, _body) = Request::builder().body(()).unwrap().into_parts();
        let result = LastEventId::from_request_parts(&mut parts, &()).await;
        let last_id = result.unwrap();
        assert_eq!(last_id.0, None);
    }
}
