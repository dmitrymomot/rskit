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

#[derive(Debug, Clone)]
pub struct RequestId(pub String);

impl RequestId {
    pub fn generate() -> Self {
        Self(ulid::Ulid::new().to_string().to_lowercase())
    }

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
            .ok_or_else(|| Error::internal("RequestId not found in request extensions"))
    }
}

pub async fn request_id_middleware(request: Request<axum::body::Body>, next: Next) -> Response {
    let (mut parts, body) = request.into_parts();

    let request_id = parts
        .headers
        .get(&REQUEST_ID_HEADER)
        .and_then(|v| v.to_str().ok())
        .filter(|v| !v.is_empty())
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
