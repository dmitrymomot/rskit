use tower_http::classify::ServerErrorsAsFailures;
use tower_http::classify::SharedClassifier;
use tower_http::trace::{MakeSpan, TraceLayer};

/// Custom span maker that includes a `tenant_id` field for tenant middleware.
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
/// The span includes a `tenant_id` field (initially empty) that the tenant
/// middleware fills in after resolution.
pub fn tracing() -> TraceLayer<SharedClassifier<ServerErrorsAsFailures>, ModoMakeSpan> {
    TraceLayer::new_for_http().make_span_with(ModoMakeSpan)
}
