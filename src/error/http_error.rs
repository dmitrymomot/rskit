//! [`HttpError`] enum for mapping well-known HTTP error codes to [`Error`].

use http::StatusCode;

use super::Error;

/// A lightweight enum of common HTTP error statuses.
///
/// Use this when you want a concise, copy-able representation of an error category without
/// allocating a message string. It converts into [`Error`] via `From<HttpError>`.
///
/// # Example
///
/// ```rust
/// use modo::error::{Error, HttpError};
///
/// let err: Error = HttpError::NotFound.into();
/// assert_eq!(err.message(), "Not Found");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpError {
    /// 400 Bad Request
    BadRequest,
    /// 401 Unauthorized
    Unauthorized,
    /// 403 Forbidden
    Forbidden,
    /// 404 Not Found
    NotFound,
    /// 405 Method Not Allowed
    MethodNotAllowed,
    /// 409 Conflict
    Conflict,
    /// 410 Gone
    Gone,
    /// 422 Unprocessable Entity
    UnprocessableEntity,
    /// 429 Too Many Requests
    TooManyRequests,
    /// 413 Payload Too Large
    PayloadTooLarge,
    /// 500 Internal Server Error
    InternalServerError,
    /// 502 Bad Gateway
    BadGateway,
    /// 503 Service Unavailable
    ServiceUnavailable,
    /// 504 Gateway Timeout
    GatewayTimeout,
}

impl HttpError {
    /// Returns the [`StatusCode`] corresponding to this variant.
    pub fn status_code(self) -> StatusCode {
        match self {
            Self::BadRequest => StatusCode::BAD_REQUEST,
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::Forbidden => StatusCode::FORBIDDEN,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::MethodNotAllowed => StatusCode::METHOD_NOT_ALLOWED,
            Self::Conflict => StatusCode::CONFLICT,
            Self::Gone => StatusCode::GONE,
            Self::UnprocessableEntity => StatusCode::UNPROCESSABLE_ENTITY,
            Self::TooManyRequests => StatusCode::TOO_MANY_REQUESTS,
            Self::PayloadTooLarge => StatusCode::PAYLOAD_TOO_LARGE,
            Self::InternalServerError => StatusCode::INTERNAL_SERVER_ERROR,
            Self::BadGateway => StatusCode::BAD_GATEWAY,
            Self::ServiceUnavailable => StatusCode::SERVICE_UNAVAILABLE,
            Self::GatewayTimeout => StatusCode::GATEWAY_TIMEOUT,
        }
    }

    /// Returns the canonical HTTP reason phrase for this variant.
    pub fn message(self) -> &'static str {
        match self {
            Self::BadRequest => "Bad Request",
            Self::Unauthorized => "Unauthorized",
            Self::Forbidden => "Forbidden",
            Self::NotFound => "Not Found",
            Self::MethodNotAllowed => "Method Not Allowed",
            Self::Conflict => "Conflict",
            Self::Gone => "Gone",
            Self::UnprocessableEntity => "Unprocessable Entity",
            Self::TooManyRequests => "Too Many Requests",
            Self::PayloadTooLarge => "Payload Too Large",
            Self::InternalServerError => "Internal Server Error",
            Self::BadGateway => "Bad Gateway",
            Self::ServiceUnavailable => "Service Unavailable",
            Self::GatewayTimeout => "Gateway Timeout",
        }
    }
}

/// Converts an [`HttpError`] into an [`Error`] using its canonical status code and message.
impl From<HttpError> for Error {
    fn from(http_err: HttpError) -> Self {
        Error::new(http_err.status_code(), http_err.message())
    }
}
