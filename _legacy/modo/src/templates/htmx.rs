use crate::app::AppState;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::{HeaderMap, HeaderValue};
use axum::response::{IntoResponse, Response};

/// Extractor for HTMX request headers. Never fails — non-HTMX requests get default values.
#[derive(Debug, Clone, Default)]
pub struct HtmxRequest {
    pub is_htmx: bool,
    pub target: Option<String>,
    pub trigger: Option<String>,
    pub trigger_name: Option<String>,
    pub current_url: Option<String>,
    pub boosted: bool,
}

impl FromRequestParts<AppState> for HtmxRequest {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        _state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let headers = &parts.headers;

        Ok(HtmxRequest {
            is_htmx: header_is_true(headers, "HX-Request"),
            target: header_str(headers, "HX-Target"),
            trigger: header_str(headers, "HX-Trigger"),
            trigger_name: header_str(headers, "HX-Trigger-Name"),
            current_url: header_str(headers, "HX-Current-URL"),
            boosted: header_is_true(headers, "HX-Boosted"),
        })
    }
}

fn header_str(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
}

fn header_is_true(headers: &HeaderMap, name: &str) -> bool {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v == "true")
}

/// Builder for HTMX response headers.
pub struct HtmxResponse<T: IntoResponse> {
    inner: T,
    headers: HeaderMap,
}

impl<T: IntoResponse> HtmxResponse<T> {
    pub fn new(inner: T) -> Self {
        Self {
            inner,
            headers: HeaderMap::new(),
        }
    }

    pub fn trigger(mut self, event: &str) -> Self {
        self.headers
            .insert("HX-Trigger", HeaderValue::from_str(event).unwrap());
        self
    }

    pub fn push_url(mut self, url: &str) -> Self {
        self.headers
            .insert("HX-Push-Url", HeaderValue::from_str(url).unwrap());
        self
    }

    pub fn reswap(mut self, strategy: &str) -> Self {
        self.headers
            .insert("HX-Reswap", HeaderValue::from_str(strategy).unwrap());
        self
    }

    pub fn retarget(mut self, selector: &str) -> Self {
        self.headers
            .insert("HX-Retarget", HeaderValue::from_str(selector).unwrap());
        self
    }

    pub fn refresh(mut self) -> Self {
        self.headers
            .insert("HX-Refresh", HeaderValue::from_static("true"));
        self
    }

    /// HTMX client-side redirect (no body).
    pub fn redirect(url: &str) -> HtmxResponse<()> {
        let mut headers = HeaderMap::new();
        headers.insert("HX-Redirect", HeaderValue::from_str(url).unwrap());
        HtmxResponse { inner: (), headers }
    }
}

impl<T: IntoResponse> IntoResponse for HtmxResponse<T> {
    fn into_response(self) -> Response {
        let mut response = self.inner.into_response();
        response.headers_mut().extend(self.headers);
        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn htmx_response_sets_headers() {
        let resp = HtmxResponse::new("body")
            .trigger("myEvent")
            .push_url("/new")
            .reswap("outerHTML")
            .retarget("#target")
            .into_response();

        assert_eq!(resp.headers().get("HX-Trigger").unwrap(), "myEvent");
        assert_eq!(resp.headers().get("HX-Push-Url").unwrap(), "/new");
        assert_eq!(resp.headers().get("HX-Reswap").unwrap(), "outerHTML");
        assert_eq!(resp.headers().get("HX-Retarget").unwrap(), "#target");
    }

    #[test]
    fn htmx_response_refresh() {
        let resp = HtmxResponse::new("ok").refresh().into_response();
        assert_eq!(resp.headers().get("HX-Refresh").unwrap(), "true");
    }

    #[test]
    fn htmx_response_redirect() {
        let resp = HtmxResponse::<()>::redirect("/login").into_response();
        assert_eq!(resp.headers().get("HX-Redirect").unwrap(), "/login");
    }
}
