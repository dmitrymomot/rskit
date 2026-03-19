use tower_http::classify::ServerErrorsAsFailures;
use tower_http::classify::SharedClassifier;
use tower_http::trace::TraceLayer;

/// Returns a tracing layer configured for HTTP request/response lifecycle logging.
///
/// Uses tower-http's default classification, which marks 5xx responses as failures.
pub fn tracing() -> TraceLayer<SharedClassifier<ServerErrorsAsFailures>> {
    TraceLayer::new_for_http()
}
