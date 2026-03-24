use axum::response::{IntoResponse, Response};
use http::StatusCode;
use std::fmt;

pub type Result<T> = std::result::Result<T, Error>;

pub struct Error {
    status: StatusCode,
    message: String,
    source: Option<Box<dyn std::error::Error + Send + Sync>>,
    error_code: Option<&'static str>,
    details: Option<serde_json::Value>,
    lagged: bool,
}

impl Error {
    pub fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
            source: None,
            error_code: None,
            details: None,
            lagged: false,
        }
    }

    pub fn with_source(
        status: StatusCode,
        message: impl Into<String>,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self {
            status,
            message: message.into(),
            source: Some(Box::new(source)),
            error_code: None,
            details: None,
            lagged: false,
        }
    }

    pub fn status(&self) -> StatusCode {
        self.status
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn details(&self) -> Option<&serde_json::Value> {
        self.details.as_ref()
    }

    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = Some(details);
        self
    }

    /// Attach a source error (builder-style).
    pub fn chain(mut self, source: impl std::error::Error + Send + Sync + 'static) -> Self {
        self.source = Some(Box::new(source));
        self
    }

    /// Attach a static error code to preserve error identity through the response pipeline.
    pub fn with_code(mut self, code: &'static str) -> Self {
        self.error_code = Some(code);
        self
    }

    /// Returns the error code, if one was set.
    pub fn error_code(&self) -> Option<&str> {
        self.error_code
    }

    /// Downcast the source error to a concrete type.
    pub fn source_as<T: std::error::Error + 'static>(&self) -> Option<&T> {
        self.source.as_ref()?.downcast_ref::<T>()
    }

    pub fn bad_request(msg: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, msg)
    }

    pub fn unauthorized(msg: impl Into<String>) -> Self {
        Self::new(StatusCode::UNAUTHORIZED, msg)
    }

    pub fn forbidden(msg: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, msg)
    }

    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, msg)
    }

    pub fn conflict(msg: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, msg)
    }

    pub fn unprocessable_entity(msg: impl Into<String>) -> Self {
        Self::new(StatusCode::UNPROCESSABLE_ENTITY, msg)
    }

    pub fn too_many_requests(msg: impl Into<String>) -> Self {
        Self::new(StatusCode::TOO_MANY_REQUESTS, msg)
    }

    pub fn payload_too_large(msg: impl Into<String>) -> Self {
        Self::new(StatusCode::PAYLOAD_TOO_LARGE, msg)
    }

    pub fn internal(msg: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, msg)
    }

    pub fn bad_gateway(msg: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_GATEWAY, msg)
    }

    pub fn gateway_timeout(msg: impl Into<String>) -> Self {
        Self::new(StatusCode::GATEWAY_TIMEOUT, msg)
    }

    /// Create an error indicating a broadcast subscriber lagged behind.
    pub fn lagged(skipped: u64) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: format!("SSE subscriber lagged, skipped {skipped} messages"),
            source: None,
            error_code: None,
            details: None,
            lagged: true,
        }
    }

    /// Returns `true` if this error represents a broadcast lag.
    pub fn is_lagged(&self) -> bool {
        self.lagged
    }
}

impl Clone for Error {
    fn clone(&self) -> Self {
        Self {
            status: self.status,
            message: self.message.clone(),
            source: None, // source (Box<dyn Error>) can't be cloned
            error_code: self.error_code,
            details: self.details.clone(),
            lagged: self.lagged,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Error")
            .field("status", &self.status)
            .field("message", &self.message)
            .field("source", &self.source)
            .field("error_code", &self.error_code)
            .field("details", &self.details)
            .field("lagged", &self.lagged)
            .finish()
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source
            .as_ref()
            .map(|e| e.as_ref() as &(dyn std::error::Error + 'static))
    }
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let status = self.status;
        let message = self.message.clone();
        let details = self.details.clone();

        let mut body = serde_json::json!({
            "error": {
                "status": status.as_u16(),
                "message": &message
            }
        });
        if let Some(ref d) = details {
            body["error"]["details"] = d.clone();
        }

        // Store a copy in extensions so error_handler middleware can read it
        let ext_error = Error {
            status,
            message,
            source: None, // source can't be cloned
            error_code: self.error_code,
            details,
            lagged: self.lagged,
        };

        let mut response = (status, axum::Json(body)).into_response();
        response.extensions_mut().insert(ext_error);
        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lagged_error_has_internal_status() {
        let err = Error::lagged(5);
        assert_eq!(err.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert!(err.message().contains('5'));
    }

    #[test]
    fn is_lagged_returns_true_for_lagged_error() {
        let err = Error::lagged(10);
        assert!(err.is_lagged());
    }

    #[test]
    fn is_lagged_returns_false_for_other_errors() {
        let err = Error::internal("something else");
        assert!(!err.is_lagged());
    }

    #[test]
    fn payload_too_large_error_has_413_status() {
        let err = Error::payload_too_large("file too big");
        assert_eq!(err.status(), StatusCode::PAYLOAD_TOO_LARGE);
        assert_eq!(err.message(), "file too big");
    }

    #[test]
    fn chain_sets_source() {
        use std::error::Error as _;
        use std::io;
        let err = super::Error::internal("failed").chain(io::Error::other("disk"));
        assert!(err.source().is_some());
    }

    #[test]
    fn source_as_downcasts_correctly() {
        use std::io;
        let io_err = io::Error::new(io::ErrorKind::NotFound, "missing");
        let err = Error::internal("failed").chain(io_err);
        let downcasted = err.source_as::<io::Error>();
        assert!(downcasted.is_some());
        assert_eq!(downcasted.unwrap().kind(), io::ErrorKind::NotFound);
    }

    #[test]
    fn source_as_returns_none_for_wrong_type() {
        use std::io;
        let err = Error::internal("failed").chain(io::Error::other("x"));
        let downcasted = err.source_as::<std::num::ParseIntError>();
        assert!(downcasted.is_none());
    }

    #[test]
    fn source_as_returns_none_when_no_source() {
        let err = Error::internal("no source");
        let downcasted = err.source_as::<std::io::Error>();
        assert!(downcasted.is_none());
    }

    #[test]
    fn with_code_sets_error_code() {
        let err = Error::unauthorized("denied").with_code("jwt:expired");
        assert_eq!(err.error_code(), Some("jwt:expired"));
    }

    #[test]
    fn error_code_is_none_by_default() {
        let err = Error::internal("plain");
        assert!(err.error_code().is_none());
    }

    #[test]
    fn error_code_survives_clone() {
        let err = Error::unauthorized("denied").with_code("jwt:expired");
        let cloned = err.clone();
        assert_eq!(cloned.error_code(), Some("jwt:expired"));
    }

    #[test]
    fn error_code_survives_into_response() {
        use axum::response::IntoResponse;
        let err = Error::unauthorized("denied").with_code("jwt:expired");
        let response = err.into_response();
        let ext_err = response.extensions().get::<Error>().unwrap();
        assert_eq!(ext_err.error_code(), Some("jwt:expired"));
    }

    #[test]
    fn bad_gateway_error_has_502_status() {
        let err = Error::bad_gateway("upstream failed");
        assert_eq!(err.status(), StatusCode::BAD_GATEWAY);
        assert_eq!(err.message(), "upstream failed");
    }

    #[test]
    fn gateway_timeout_error_has_504_status() {
        let err = Error::gateway_timeout("timed out");
        assert_eq!(err.status(), StatusCode::GATEWAY_TIMEOUT);
        assert_eq!(err.message(), "timed out");
    }
}
