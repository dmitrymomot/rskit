use axum::response::{Html, IntoResponse, Response};
use http::{HeaderValue, StatusCode};

/// Opaque response type returned by `ViewRenderer` methods.
/// Can hold rendered HTML or a redirect.
pub struct ViewResponse {
    kind: ViewResponseKind,
}

enum ViewResponseKind {
    Html { body: String, vary: bool },
    Redirect { url: String, status: StatusCode },
    HxRedirect { url: String },
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
        Self::redirect_with_status(url, StatusCode::FOUND)
    }

    /// Create a redirect with a specific HTTP status code (301, 302, 303, 307, 308).
    pub fn redirect_with_status(url: impl Into<String>, status: StatusCode) -> Self {
        Self {
            kind: ViewResponseKind::Redirect {
                url: url.into(),
                status,
            },
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
            ViewResponseKind::Redirect { url, status } => match HeaderValue::try_from(&url) {
                Ok(val) => {
                    let mut resp = Response::new(axum::body::Body::empty());
                    *resp.status_mut() = status;
                    resp.headers_mut().insert("location", val);
                    resp
                }
                Err(_) => {
                    tracing::error!("Invalid redirect URL");
                    StatusCode::INTERNAL_SERVER_ERROR.into_response()
                }
            },
            ViewResponseKind::HxRedirect { url } => match HeaderValue::try_from(&url) {
                Ok(val) => {
                    let mut resp = Response::new(axum::body::Body::empty());
                    *resp.status_mut() = StatusCode::OK;
                    resp.headers_mut().insert("hx-redirect", val);
                    resp
                }
                Err(_) => {
                    tracing::error!("Invalid HX-Redirect URL");
                    StatusCode::INTERNAL_SERVER_ERROR.into_response()
                }
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::StatusCode;

    #[test]
    fn test_redirect_defaults_to_302() {
        let resp = ViewResponse::redirect("/foo").into_response();
        assert_eq!(resp.status(), StatusCode::FOUND);
        assert_eq!(resp.headers().get("location").unwrap(), "/foo");
    }

    #[test]
    fn test_redirect_with_status_301() {
        let resp = ViewResponse::redirect_with_status("/moved", StatusCode::MOVED_PERMANENTLY)
            .into_response();
        assert_eq!(resp.status(), StatusCode::MOVED_PERMANENTLY);
        assert_eq!(resp.headers().get("location").unwrap(), "/moved");
    }

    #[test]
    fn test_redirect_with_status_303() {
        let resp =
            ViewResponse::redirect_with_status("/see-other", StatusCode::SEE_OTHER).into_response();
        assert_eq!(resp.status(), StatusCode::SEE_OTHER);
        assert_eq!(resp.headers().get("location").unwrap(), "/see-other");
    }

    #[test]
    fn test_redirect_with_status_307() {
        let resp = ViewResponse::redirect_with_status("/temp", StatusCode::TEMPORARY_REDIRECT)
            .into_response();
        assert_eq!(resp.status(), StatusCode::TEMPORARY_REDIRECT);
        assert_eq!(resp.headers().get("location").unwrap(), "/temp");
    }
}
