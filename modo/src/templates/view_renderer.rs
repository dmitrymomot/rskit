use std::sync::Arc;

use axum::extract::FromRequestParts;
use http::StatusCode;
use http::request::Parts;

use crate::error::Error;
use crate::templates::view_response::ViewResponse;
use crate::templates::{TemplateContext, TemplateEngine, ViewRender};

/// Request-scoped extractor for explicit template rendering.
///
/// Combines the `TemplateEngine`, `TemplateContext`, and HTMX detection.
/// Use this when you need to compose multiple views, return different view
/// types from different branches, or perform smart redirects.
pub struct ViewRenderer {
    engine: Arc<TemplateEngine>,
    context: TemplateContext,
    is_htmx: bool,
}

impl ViewRenderer {
    /// Render one or more views into an HTTP response.
    ///
    /// Accepts a single `#[view]` struct or a tuple of views.
    /// Multiple views are rendered independently and concatenated.
    /// Adds `Vary: HX-Request` header when any view has a dual template.
    pub fn render(&self, views: impl ViewRender) -> Result<ViewResponse, Error> {
        let has_dual = views.has_dual_template();
        let html = views.render_with(&self.engine, &self.context, self.is_htmx)?;
        if has_dual {
            Ok(ViewResponse::html_with_vary(html))
        } else {
            Ok(ViewResponse::html(html))
        }
    }

    /// Smart redirect — returns 302 for normal requests,
    /// `HX-Redirect` header + 200 for HTMX requests.
    ///
    /// Returns `Result` for ergonomic consistency with `render()` so handlers
    /// can use `?` uniformly across all branches without wrapping in `Ok(...)`.
    pub fn redirect(&self, url: &str) -> Result<ViewResponse, Error> {
        if self.is_htmx {
            Ok(ViewResponse::hx_redirect(url))
        } else {
            Ok(ViewResponse::redirect(url))
        }
    }

    /// Smart redirect with custom status — returns redirect with given status
    /// for normal requests, `HX-Redirect` header + 200 for HTMX requests.
    pub fn redirect_with_status(
        &self,
        url: &str,
        status: StatusCode,
    ) -> Result<ViewResponse, Error> {
        if self.is_htmx {
            Ok(ViewResponse::hx_redirect(url))
        } else {
            Ok(ViewResponse::redirect_with_status(url, status))
        }
    }

    /// Render a view to a plain `String`.
    ///
    /// Useful for non-HTTP contexts: SSE events, WebSocket messages, emails.
    /// Always uses the main template (not the HTMX partial).
    pub fn render_to_string(&self, view: impl ViewRender) -> Result<String, Error> {
        view.render_with(&self.engine, &self.context, false)
            .map_err(Into::into)
    }

    /// Whether this is an HTMX request (`HX-Request` header present).
    pub fn is_htmx(&self) -> bool {
        self.is_htmx
    }
}

impl<S: Send + Sync> FromRequestParts<S> for ViewRenderer {
    type Rejection = Error;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let engine = parts
            .extensions
            .get::<Arc<TemplateEngine>>()
            .cloned()
            .ok_or_else(|| {
                Error::internal(
                    "view renderer requires TemplateEngine \
                     — register it as a service or add Extension(Arc::new(engine))",
                )
            })?;

        let context = parts
            .extensions
            .get::<TemplateContext>()
            .cloned()
            .unwrap_or_else(|| {
                tracing::warn!(
                    "TemplateContext not found in request extensions. \
                     Ensure TemplateContextLayer is applied."
                );
                TemplateContext::default()
            });

        let is_htmx = parts.headers.get("hx-request").is_some();

        Ok(Self {
            engine,
            context,
            is_htmx,
        })
    }
}
