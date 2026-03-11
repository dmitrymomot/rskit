use axum::response::{Html, IntoResponse, Response};
use http::{HeaderValue, StatusCode};

/// Opaque response type returned by `ViewRenderer` methods.
/// Can hold rendered HTML or a redirect.
pub struct ViewResponse {
    kind: ViewResponseKind,
}

enum ViewResponseKind {
    Html {
        body: String,
        vary: bool,
    },
    Redirect {
        url: String,
    },
    HxRedirect {
        url: String,
    },
}

impl ViewResponse {
    /// Create an HTML response with 200 status.
    pub fn html(body: String) -> Self {
        Self {
            kind: ViewResponseKind::Html { body, vary: false },
        }
    }

    /// Create an HTML response with `Vary: HX-Request` header.
    pub fn html_with_vary(body: String) -> Self {
        Self {
            kind: ViewResponseKind::Html { body, vary: true },
        }
    }

    /// Create a standard 302 redirect.
    pub fn redirect(url: impl Into<String>) -> Self {
        Self {
            kind: ViewResponseKind::Redirect { url: url.into() },
        }
    }

    /// Create an HTMX-aware redirect (200 + HX-Redirect header).
    pub fn hx_redirect(url: impl Into<String>) -> Self {
        Self {
            kind: ViewResponseKind::HxRedirect { url: url.into() },
        }
    }
}

impl IntoResponse for ViewResponse {
    fn into_response(self) -> Response {
        match self.kind {
            ViewResponseKind::Html { body, vary } => {
                let mut resp = Html(body).into_response();
                if vary {
                    resp.headers_mut()
                        .insert("vary", HeaderValue::from_static("HX-Request"));
                }
                resp
            }
            ViewResponseKind::Redirect { url } => {
                let mut resp = Response::new(axum::body::Body::empty());
                *resp.status_mut() = StatusCode::FOUND;
                if let Ok(val) = HeaderValue::from_str(&url) {
                    resp.headers_mut().insert("location", val);
                }
                resp
            }
            ViewResponseKind::HxRedirect { url } => {
                let mut resp = Response::new(axum::body::Body::empty());
                *resp.status_mut() = StatusCode::OK;
                if let Ok(val) = HeaderValue::from_str(&url) {
                    resp.headers_mut().insert("hx-redirect", val);
                }
                resp
            }
        }
    }
}
