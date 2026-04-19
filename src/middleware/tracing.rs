use tower_http::classify::ServerErrorsAsFailures;
use tower_http::classify::SharedClassifier;
use tower_http::trace::{MakeSpan, TraceLayer};

/// Span maker that creates an `http_request` tracing span for each request.
///
/// Includes a `tenant_id` field (initially empty) so that the tenant
/// middleware can record it via `span.record("tenant_id", ...)` after
/// resolving the tenant.
#[derive(Clone, Debug)]
pub struct ModoMakeSpan;

impl<B> MakeSpan<B> for ModoMakeSpan {
    fn make_span(&mut self, request: &http::Request<B>) -> tracing::Span {
        tracing::info_span!(
            "http_request",
            method = %request.method(),
            uri = %request.uri(),
            version = ?request.version(),
            tenant_id = tracing::field::Empty,
        )
    }
}

/// Returns a tracing layer configured for HTTP request/response lifecycle logging.
///
/// The span name is `http_request` and the initial fields are `method`,
/// `uri`, `version`, and `tenant_id` (empty — filled in by the tenant
/// middleware via [`tracing::Span::record`] after tenant resolution).
/// Any middleware that needs to record a span field must add it to
/// [`ModoMakeSpan`] first; fields not pre-declared are dropped by
/// `tracing`.
///
/// # Layer ordering
///
/// Install `tracing()` **outermost** (the last `.layer(...)` call in your
/// chain) so every inbound request — including those rejected by
/// [`csrf`](super::csrf) / [`rate_limit`](super::rate_limit) — is
/// observed inside the span.
///
/// # Example
///
/// ```rust,no_run
/// use axum::{Router, routing::get};
/// use modo::middleware::tracing;
///
/// let app: Router = Router::new()
///     .route("/", get(|| async { "ok" }))
///     .layer(tracing());
/// ```
pub fn tracing() -> TraceLayer<SharedClassifier<ServerErrorsAsFailures>, ModoMakeSpan> {
    TraceLayer::new_for_http().make_span_with(ModoMakeSpan)
}
