use axum::extract::FromRequestParts;
use http::request::Parts;

#[derive(Debug, Clone, Copy)]
pub struct HxRequest(bool);

impl HxRequest {
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
}
