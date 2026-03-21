use axum::response::{IntoResponse, Response};
use http::StatusCode;
use std::fmt;

pub type Result<T> = std::result::Result<T, Error>;

pub struct Error {
    status: StatusCode,
    message: String,
    source: Option<Box<dyn std::error::Error + Send + Sync>>,
    details: Option<serde_json::Value>,
    lagged: bool,
}

impl Error {
    pub fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
            source: None,
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

    pub fn internal(msg: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, msg)
    }

    /// Create an error indicating a broadcast subscriber lagged behind.
    pub fn lagged(skipped: u64) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: format!("SSE subscriber lagged, skipped {skipped} messages"),
            source: None,
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
}
