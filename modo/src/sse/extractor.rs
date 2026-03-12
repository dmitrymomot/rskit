use super::config::SseConfig;
use super::event::SseEvent;
use super::response::SseResponse;
use super::sender::SseSender;
use crate::app::AppState;
use crate::error::Error;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use futures_util::Stream;

/// SSE extractor that auto-applies [`SseConfig`] from the service registry.
///
/// Use this in handlers to create SSE responses with the application's
/// configured keep-alive interval instead of the hardcoded default.
///
/// # Example
///
/// ```rust,ignore
/// use modo::sse::{Sse, SseEvent, SseResponse, SseStreamExt};
///
/// #[modo::handler(GET, "/events")]
/// async fn events(
///     sse: Sse,
///     Service(bc): Service<SseBroadcastManager<String, MyEvent>>,
/// ) -> SseResponse {
///     sse.from_stream(bc.subscribe(&"topic".into()).sse_json())
/// }
/// ```
///
/// # Config
///
/// The keep-alive interval is read from `sse.keep_alive_interval_secs` in
/// your application config YAML. Falls back to 15 seconds if not configured.
///
/// ```yaml
/// sse:
///     keep_alive_interval_secs: 30
/// ```
pub struct Sse {
    config: SseConfig,
}

impl FromRequestParts<AppState> for Sse {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        _parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let config = state
            .services
            .get::<SseConfig>()
            .map(|arc| (*arc).clone())
            .unwrap_or_default();
        Ok(Sse { config })
    }
}

impl Sse {
    /// Create an [`SseResponse`] from any stream of [`SseEvent`]s.
    ///
    /// Applies the configured keep-alive interval automatically.
    /// Chain [`.with_keep_alive()`](SseResponse::with_keep_alive) to override
    /// for a specific handler.
    pub fn from_stream<S, E>(&self, stream: S) -> SseResponse
    where
        S: Stream<Item = Result<SseEvent, E>> + Send + 'static,
        E: Into<Error> + Send + 'static,
    {
        super::response::from_stream(stream).with_keep_alive(self.config.keep_alive_interval())
    }

    /// Create an [`SseResponse`] with an imperative sender.
    ///
    /// Applies the configured keep-alive interval automatically.
    /// See [`channel()`](super::channel) for full documentation.
    pub fn channel<F, Fut>(&self, f: F) -> SseResponse
    where
        F: FnOnce(SseSender) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = Result<(), Error>> + Send,
    {
        super::sender::channel(f).with_keep_alive(self.config.keep_alive_interval())
    }
}
