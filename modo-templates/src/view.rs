use axum::response::{IntoResponse, Response};
use http::StatusCode;
use minijinja::Value;

/// A pending template render. Created by the `#[view]` macro's `IntoResponse` impl.
/// The render layer middleware picks this up from response extensions and renders it.
#[derive(Debug, Clone)]
pub struct View {
    /// Primary template path (full page).
    pub template: String,
    /// Optional HTMX template path (fragment).
    pub htmx_template: Option<String>,
    /// Serialized user context (struct fields).
    pub user_context: Value,
    /// HTTP status code for the response.
    pub status: StatusCode,
}

impl View {
    pub fn new(template: impl Into<String>, user_context: Value) -> Self {
        Self {
            template: template.into(),
            htmx_template: None,
            user_context,
            status: StatusCode::OK,
        }
    }

    pub fn with_htmx(mut self, htmx_template: impl Into<String>) -> Self {
        self.htmx_template = Some(htmx_template.into());
        self
    }

    pub fn with_status(mut self, status: StatusCode) -> Self {
        self.status = status;
        self
    }
}

/// Marker response: stashes the View in response extensions for the render layer.
impl IntoResponse for View {
    fn into_response(self) -> Response {
        let mut response = Response::new(axum::body::Body::empty());
        *response.status_mut() = self.status;
        response.extensions_mut().insert(self);
        response
    }
}
