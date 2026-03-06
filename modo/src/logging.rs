use crate::request_id::RequestId;
use axum::http::Request;
use tower_http::classify::{ServerErrorsAsFailures, SharedClassifier};
use tower_http::trace::{
    DefaultOnBodyChunk, DefaultOnEos, DefaultOnFailure, DefaultOnRequest, DefaultOnResponse,
    MakeSpan, TraceLayer,
};
use tracing::{Level, Span};

#[derive(Clone, Debug)]
pub struct ModoMakeSpan {
    level: Level,
}

impl ModoMakeSpan {
    pub fn new(level: Level) -> Self {
        Self { level }
    }
}

impl<B> MakeSpan<B> for ModoMakeSpan {
    fn make_span(&mut self, request: &Request<B>) -> Span {
        let request_id = request
            .extensions()
            .get::<RequestId>()
            .map(|r| r.0.as_str())
            .unwrap_or("");

        macro_rules! make_span {
            ($level:expr) => {
                tracing::span!(
                    $level,
                    "http_request",
                    method = %request.method(),
                    uri = %request.uri(),
                    version = ?request.version(),
                    request_id = %request_id,
                )
            };
        }

        match self.level {
            Level::TRACE => make_span!(Level::TRACE),
            Level::DEBUG => make_span!(Level::DEBUG),
            Level::INFO => make_span!(Level::INFO),
            Level::WARN => make_span!(Level::WARN),
            Level::ERROR => make_span!(Level::ERROR),
        }
    }
}

pub fn parse_level(s: &str) -> Level {
    match s.to_lowercase().as_str() {
        "trace" => Level::TRACE,
        "debug" => Level::DEBUG,
        "info" => Level::INFO,
        "warn" => Level::WARN,
        "error" => Level::ERROR,
        _ => Level::INFO,
    }
}

pub fn trace_layer(
    level: Level,
) -> TraceLayer<
    SharedClassifier<ServerErrorsAsFailures>,
    ModoMakeSpan,
    DefaultOnRequest,
    DefaultOnResponse,
    DefaultOnBodyChunk,
    DefaultOnEos,
    DefaultOnFailure,
> {
    TraceLayer::new_for_http()
        .make_span_with(ModoMakeSpan::new(level))
        .on_request(DefaultOnRequest::new().level(level))
        .on_response(DefaultOnResponse::new().level(level))
}
