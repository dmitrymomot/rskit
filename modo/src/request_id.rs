use crate::app::AppState;
use crate::error::Error;
use axum::extract::FromRequestParts;
use axum::http::header::HeaderName;
use axum::http::request::Parts;
use axum::http::{HeaderValue, Request};
use axum::middleware::Next;
use axum::response::Response;
use std::fmt;

const REQUEST_ID_HEADER: HeaderName = HeaderName::from_static("x-request-id");

/// A per-request ULID identifier injected by the request ID middleware.
///
/// Propagated via the `X-Request-ID` request and response headers.
/// Extract in handlers to correlate logs with client requests.
#[derive(Debug, Clone)]
pub struct RequestId(pub String);

impl RequestId {
    /// Generate a new ULID-based request ID (lowercase).
    pub fn generate() -> Self {
        Self(ulid::Ulid::new().to_string().to_lowercase())
    }

    /// Return the ID as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for RequestId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromRequestParts<AppState> for RequestId {
    type Rejection = Error;

    async fn from_request_parts(
        parts: &mut Parts,
        _state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<RequestId>()
            .cloned()
            .ok_or_else(|| Error::internal("request ID not found in request extensions"))
    }
}

fn is_valid_request_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

pub async fn request_id_middleware(request: Request<axum::body::Body>, next: Next) -> Response {
    let (mut parts, body) = request.into_parts();

    let request_id = parts
        .headers
        .get(&REQUEST_ID_HEADER)
        .and_then(|v| v.to_str().ok())
        .filter(|v| is_valid_request_id(v))
        .map(|v| RequestId(v.to_string()))
        .unwrap_or_else(RequestId::generate);

    parts.extensions.insert(request_id.clone());

    let request = Request::from_parts(parts, body);
    let mut response = next.run(request).await;

    if let Ok(value) = HeaderValue::from_str(request_id.as_str()) {
        response.headers_mut().insert(REQUEST_ID_HEADER, value);
    }

    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::Router;
    use axum::routing::get;
    use http::Request;
    use tower::ServiceExt;

    fn test_app() -> Router {
        Router::new()
            .route("/", get(|| async { "ok" }))
            .layer(axum::middleware::from_fn(request_id_middleware))
    }

    #[test]
    fn valid_ids() {
        assert!(is_valid_request_id("abc123"));
        assert!(is_valid_request_id("request-id-123"));
        assert!(is_valid_request_id("request_id_123"));
        assert!(is_valid_request_id("550e8400-e29b-41d4-a716-446655440000")); // UUID
        assert!(is_valid_request_id(&"a".repeat(128))); // boundary
    }

    #[test]
    fn invalid_ids() {
        assert!(!is_valid_request_id(""));
        assert!(!is_valid_request_id(&"a".repeat(129))); // too long
        assert!(!is_valid_request_id("has spaces"));
        assert!(!is_valid_request_id("has;semicolons"));
        assert!(!is_valid_request_id("has.dots"));
        assert!(!is_valid_request_id("has/slashes"));
        assert!(!is_valid_request_id("injection\r\nheader: value"));
    }

    #[tokio::test]
    async fn valid_request_id_is_propagated() {
        let app = test_app();
        let request = Request::builder()
            .uri("/")
            .header("x-request-id", "my-custom-id-123")
            .body(axum::body::Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(
            response
                .headers()
                .get("x-request-id")
                .unwrap()
                .to_str()
                .unwrap(),
            "my-custom-id-123"
        );
    }

    #[tokio::test]
    async fn missing_header_generates_new_id() {
        let app = test_app();
        let request = Request::builder()
            .uri("/")
            .body(axum::body::Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        let id = response
            .headers()
            .get("x-request-id")
            .unwrap()
            .to_str()
            .unwrap();
        assert!(!id.is_empty());
        assert_eq!(id.len(), 26); // ULID length
    }

    #[tokio::test]
    async fn empty_header_generates_new_id() {
        let app = test_app();
        let request = Request::builder()
            .uri("/")
            .header("x-request-id", "")
            .body(axum::body::Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        let id = response
            .headers()
            .get("x-request-id")
            .unwrap()
            .to_str()
            .unwrap();
        assert_ne!(id, "");
        assert_eq!(id.len(), 26);
    }

    #[tokio::test]
    async fn oversized_id_generates_new() {
        let app = test_app();
        let long_id = "a".repeat(129);
        let request = Request::builder()
            .uri("/")
            .header("x-request-id", &long_id)
            .body(axum::body::Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        let id = response
            .headers()
            .get("x-request-id")
            .unwrap()
            .to_str()
            .unwrap();
        assert_ne!(id, long_id);
        assert_eq!(id.len(), 26);
    }

    #[tokio::test]
    async fn special_chars_rejected() {
        let app = test_app();
        let request = Request::builder()
            .uri("/")
            .header("x-request-id", "bad;id")
            .body(axum::body::Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        let id = response
            .headers()
            .get("x-request-id")
            .unwrap()
            .to_str()
            .unwrap();
        assert_ne!(id, "bad;id");
        assert_eq!(id.len(), 26);
    }
}
