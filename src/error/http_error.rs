use http::StatusCode;

use super::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpError {
    BadRequest,
    Unauthorized,
    Forbidden,
    NotFound,
    MethodNotAllowed,
    Conflict,
    Gone,
    UnprocessableEntity,
    TooManyRequests,
    InternalServerError,
    BadGateway,
    ServiceUnavailable,
    GatewayTimeout,
}

impl HttpError {
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
            Self::InternalServerError => StatusCode::INTERNAL_SERVER_ERROR,
            Self::BadGateway => StatusCode::BAD_GATEWAY,
            Self::ServiceUnavailable => StatusCode::SERVICE_UNAVAILABLE,
            Self::GatewayTimeout => StatusCode::GATEWAY_TIMEOUT,
        }
    }

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
            Self::InternalServerError => "Internal Server Error",
            Self::BadGateway => "Bad Gateway",
            Self::ServiceUnavailable => "Service Unavailable",
            Self::GatewayTimeout => "Gateway Timeout",
        }
    }
}

impl From<HttpError> for Error {
    fn from(http_err: HttpError) -> Self {
        Error::new(http_err.status_code(), http_err.message())
    }
}
