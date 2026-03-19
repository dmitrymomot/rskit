use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use http::{HeaderName, HeaderValue, Response};
use serde::Deserialize;
use tower::{Layer, Service};

/// Configuration for security response headers.
///
/// All fields have sensible defaults. Optional fields (`hsts_max_age`,
/// `content_security_policy`, `permissions_policy`) are `None` by default
/// and their corresponding headers are only added when set.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct SecurityHeadersConfig {
    /// When `true`, adds `X-Content-Type-Options: nosniff`.
    pub x_content_type_options: bool,
    /// Value for the `X-Frame-Options` header (e.g. `"DENY"`, `"SAMEORIGIN"`).
    pub x_frame_options: String,
    /// Value for the `Referrer-Policy` header.
    pub referrer_policy: String,
    /// When set, adds `Strict-Transport-Security: max-age=<value>`.
    pub hsts_max_age: Option<u64>,
    /// When set, adds the `Content-Security-Policy` header with this value.
    pub content_security_policy: Option<String>,
    /// When set, adds the `Permissions-Policy` header with this value.
    pub permissions_policy: Option<String>,
}

impl Default for SecurityHeadersConfig {
    fn default() -> Self {
        Self {
            x_content_type_options: true,
            x_frame_options: "DENY".to_string(),
            referrer_policy: "strict-origin-when-cross-origin".to_string(),
            hsts_max_age: None,
            content_security_policy: None,
            permissions_policy: None,
        }
    }
}

/// A [`Layer`] that adds configurable security headers to every response.
#[derive(Clone)]
pub struct SecurityHeadersLayer {
    headers: Vec<(HeaderName, HeaderValue)>,
}

impl SecurityHeadersLayer {
    fn from_config(config: &SecurityHeadersConfig) -> Self {
        let mut headers = Vec::new();

        if config.x_content_type_options {
            headers.push((
                http::header::X_CONTENT_TYPE_OPTIONS,
                HeaderValue::from_static("nosniff"),
            ));
        }

        headers.push((
            HeaderName::from_static("x-frame-options"),
            HeaderValue::from_str(&config.x_frame_options).expect("invalid x-frame-options value"),
        ));

        headers.push((
            HeaderName::from_static("referrer-policy"),
            HeaderValue::from_str(&config.referrer_policy).expect("invalid referrer-policy value"),
        ));

        if let Some(max_age) = config.hsts_max_age {
            headers.push((
                http::header::STRICT_TRANSPORT_SECURITY,
                HeaderValue::from_str(&format!("max-age={max_age}"))
                    .expect("invalid hsts max-age value"),
            ));
        }

        if let Some(ref csp) = config.content_security_policy {
            headers.push((
                HeaderName::from_static("content-security-policy"),
                HeaderValue::from_str(csp).expect("invalid content-security-policy value"),
            ));
        }

        if let Some(ref pp) = config.permissions_policy {
            headers.push((
                HeaderName::from_static("permissions-policy"),
                HeaderValue::from_str(pp).expect("invalid permissions-policy value"),
            ));
        }

        Self { headers }
    }
}

impl<S> Layer<S> for SecurityHeadersLayer {
    type Service = SecurityHeadersService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        SecurityHeadersService {
            inner,
            headers: self.headers.clone(),
        }
    }
}

/// The [`Service`] produced by [`SecurityHeadersLayer`].
///
/// Wraps an inner service and appends security headers to every response.
#[derive(Clone)]
pub struct SecurityHeadersService<S> {
    inner: S,
    headers: Vec<(HeaderName, HeaderValue)>,
}

impl<S, ReqBody, ResBody> Service<http::Request<ReqBody>> for SecurityHeadersService<S>
where
    S: Service<http::Request<ReqBody>, Response = Response<ResBody>>,
    S::Future: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: http::Request<ReqBody>) -> Self::Future {
        let headers = self.headers.clone();
        let future = self.inner.call(request);

        Box::pin(async move {
            let mut response = future.await?;
            let resp_headers = response.headers_mut();
            for (name, value) in headers {
                resp_headers.insert(name, value);
            }
            Ok(response)
        })
    }
}

/// Returns a [`SecurityHeadersLayer`] that adds security headers to every response
/// based on the provided configuration.
///
/// # Example
///
/// ```rust,no_run
/// use modo::middleware::{security_headers, SecurityHeadersConfig};
///
/// let config = SecurityHeadersConfig::default();
/// let layer = security_headers(&config);
/// ```
pub fn security_headers(config: &SecurityHeadersConfig) -> SecurityHeadersLayer {
    SecurityHeadersLayer::from_config(config)
}
