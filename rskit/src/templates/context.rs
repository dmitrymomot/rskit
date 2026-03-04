use crate::app::AppState;
use crate::templates::flash::{FlashMessage, FlashMessages};
use axum::extract::FromRequestParts;
use axum::http::request::Parts;

/// Common context for templates. Gathers HTMX state, flash messages, CSRF token, etc.
pub struct BaseContext {
    pub request_id: String,
    pub is_htmx: bool,
    pub current_url: String,
    pub flash_messages: Vec<FlashMessage>,
    pub csrf_token: String,
    pub locale: String,
}

impl FromRequestParts<AppState> for BaseContext {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        // request_id: prefer X-Request-Id header, fallback to generated ULID
        let request_id = parts
            .headers
            .get("X-Request-Id")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string())
            .unwrap_or_else(|| ulid::Ulid::new().to_string());

        // is_htmx
        let is_htmx = parts
            .headers
            .get("HX-Request")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v == "true");

        // current_url: prefer HX-Current-URL, fallback to request URI
        let current_url = parts
            .headers
            .get("HX-Current-URL")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string())
            .unwrap_or_else(|| parts.uri.to_string());

        // flash_messages
        let FlashMessages(flash_messages) = FlashMessages::from_request_parts(parts, state).await?;

        // csrf_token from request extensions (set by csrf middleware)
        let csrf_token = parts
            .extensions
            .get::<crate::middleware::CsrfToken>()
            .map(|t| t.0.clone())
            .unwrap_or_default();

        // locale from Accept-Language header
        let locale = parts
            .headers
            .get("Accept-Language")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.split(',').next())
            .and_then(|s| s.split(';').next())
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| "en".to_string());

        Ok(BaseContext {
            request_id,
            is_htmx,
            current_url,
            flash_messages,
            csrf_token,
            locale,
        })
    }
}
