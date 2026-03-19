use http::StatusCode;

use super::Error;

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Error::with_source(StatusCode::INTERNAL_SERVER_ERROR, "IO error", err)
    }
}

impl From<serde_json::Error> for Error {
    fn from(err: serde_json::Error) -> Self {
        Error::with_source(StatusCode::BAD_REQUEST, "JSON error", err)
    }
}

impl From<serde_yaml_ng::Error> for Error {
    fn from(err: serde_yaml_ng::Error) -> Self {
        Error::with_source(StatusCode::INTERNAL_SERVER_ERROR, "YAML error", err)
    }
}
