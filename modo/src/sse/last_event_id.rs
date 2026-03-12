use axum::extract::FromRequestParts;
use axum::http::request::Parts;

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
/// responsible for fetching missed events (e.g., from a database) and sending
/// them before subscribing to the live stream.
///
/// # Example
///
/// ```rust,ignore
/// #[modo::handler(GET, "/events")]
/// async fn events(
///     last_event_id: LastEventId,
///     Service(bc): Service<SseBroadcastManager<String, MyEvent>>,
///     Db(db): Db,
/// ) -> SseResponse {
///     let stream = if let Some(last_id) = last_event_id.0 {
///         // Replay missed events from DB, then subscribe to live
///         let missed = load_events_after(&db, &last_id).await?;
///         let live = bc.subscribe(&"topic".into());
///         missed.chain(live)
///     } else {
///         bc.subscribe(&"topic".into())
///     };
///     modo::sse::from_stream(stream.sse_map(|e| Ok(SseEvent::from(e))))
/// }
/// ```
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
