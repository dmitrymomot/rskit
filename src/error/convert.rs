//! `From` conversions for common standard-library and third-party error types into [`Error`].

use http::StatusCode;

use super::Error;

/// Converts a [`std::io::Error`] into a `500 Internal Server Error`.
impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Error::with_source(StatusCode::INTERNAL_SERVER_ERROR, "IO error", err)
    }
}

/// Converts a [`serde_json::Error`] into a `400 Bad Request`.
impl From<serde_json::Error> for Error {
    fn from(err: serde_json::Error) -> Self {
        Error::with_source(StatusCode::BAD_REQUEST, "JSON error", err)
    }
}

/// Converts a [`serde_yaml_ng::Error`] into a `500 Internal Server Error`.
impl From<serde_yaml_ng::Error> for Error {
    fn from(err: serde_yaml_ng::Error) -> Self {
        Error::with_source(StatusCode::INTERNAL_SERVER_ERROR, "YAML error", err)
    }
}
