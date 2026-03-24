use axum::extract::FromRequestParts;
use http::request::Parts;

/// Axum extractor that detects HTMX requests.
///
/// Returns `HxRequest(true)` when the request contains `HX-Request: true`,
/// and `HxRequest(false)` otherwise. The extraction is infallible.
///
/// # Example
///
/// ```rust,no_run
/// use modo::template::HxRequest;
///
/// async fn handler(hx: HxRequest) {
///     if hx.is_htmx() {
///         // respond with a partial
///     }
/// }
/// ```
#[derive(Debug, Clone, Copy)]
pub struct HxRequest(bool);

impl HxRequest {
    /// Returns `true` if the request was issued by HTMX (`HX-Request: true`).
    pub fn is_htmx(&self) -> bool {
        self.0
    }
}

impl<S: Send + Sync> FromRequestParts<S> for HxRequest {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let is_htmx = parts
            .headers
            .get("hx-request")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v == "true");
        Ok(HxRequest(is_htmx))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::FromRequestParts;
    use http::Request;

    #[tokio::test]
    async fn detects_htmx_request() {
        let req = Request::builder()
            .header("hx-request", "true")
            .body(())
            .unwrap();
        let (mut parts, _) = req.into_parts();
        let hx = HxRequest::from_request_parts(&mut parts, &())
            .await
            .unwrap();
        assert!(hx.is_htmx());
    }

    #[tokio::test]
    async fn detects_non_htmx_request() {
        let req = Request::builder().body(()).unwrap();
        let (mut parts, _) = req.into_parts();
        let hx = HxRequest::from_request_parts(&mut parts, &())
            .await
            .unwrap();
        assert!(!hx.is_htmx());
    }

    #[tokio::test]
    async fn hx_request_false_header_is_not_htmx() {
        let req = Request::builder()
            .header("hx-request", "false")
            .body(())
            .unwrap();
        let (mut parts, _) = req.into_parts();
        let hx = HxRequest::from_request_parts(&mut parts, &())
            .await
            .unwrap();
        assert!(!hx.is_htmx());
    }

    #[tokio::test]
    async fn hx_request_header_case_insensitive() {
        let req = Request::builder()
            .header("HX-Request", "true")
            .body(())
            .unwrap();
        let (mut parts, _) = req.into_parts();
        let hx = HxRequest::from_request_parts(&mut parts, &())
            .await
            .unwrap();
        assert!(hx.is_htmx());
    }
}
