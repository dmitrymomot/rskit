use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use axum::body::Body;
use axum::response::IntoResponse;
use axum_extra::extract::cookie::Key;
use cookie::{Cookie, CookieJar, SameSite};
use http::{HeaderValue, Method, Request, Response, StatusCode};
use serde::Deserialize;
use tower::{Layer, Service};

/// Configuration for CSRF protection middleware.
///
/// Uses the double-submit cookie pattern: a signed HttpOnly cookie holds the
/// token, and the client must send the same token back via a header or form
/// field on state-changing requests.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct CsrfConfig {
    /// Name of the CSRF cookie.
    pub cookie_name: String,
    /// Name of the HTTP header carrying the CSRF token.
    pub header_name: String,
    /// Name of the form field carrying the CSRF token.
    pub field_name: String,
    /// Cookie time-to-live in seconds.
    pub ttl_secs: u64,
    /// HTTP methods exempt from CSRF validation.
    pub exempt_methods: Vec<String>,
}

impl Default for CsrfConfig {
    fn default() -> Self {
        Self {
            cookie_name: "_csrf".to_string(),
            header_name: "X-CSRF-Token".to_string(),
            field_name: "_csrf_token".to_string(),
            ttl_secs: 21600,
            exempt_methods: vec!["GET", "HEAD", "OPTIONS"]
                .into_iter()
                .map(String::from)
                .collect(),
        }
    }
}

/// CSRF token newtype, stored in request and response extensions for
/// handler/template access.
#[derive(Clone, Debug)]
pub struct CsrfToken(pub String);

/// A [`Layer`] that applies CSRF protection using the double-submit cookie
/// pattern with signed cookies.
#[derive(Clone)]
pub struct CsrfLayer {
    config: CsrfConfig,
    key: Key,
}

impl<S> Layer<S> for CsrfLayer {
    type Service = CsrfService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        CsrfService {
            inner,
            config: self.config.clone(),
            key: self.key.clone(),
        }
    }
}

/// The [`Service`] produced by `CsrfLayer`.
///
/// For exempt methods (GET, HEAD, OPTIONS by default), generates a new CSRF
/// token, sets a signed cookie, and injects [`CsrfToken`] into both request
/// and response extensions.
///
/// For unsafe methods (POST, PUT, DELETE, PATCH, etc.), reads the signed
/// cookie, compares the plain token with the header value, and rejects with
/// 403 Forbidden on mismatch.
#[derive(Clone)]
pub struct CsrfService<S> {
    inner: S,
    config: CsrfConfig,
    key: Key,
}

impl<S> CsrfService<S> {
    /// Signs a token and returns the signed cookie value string.
    fn sign_token(&self, token: &str) -> String {
        let mut jar = CookieJar::new();
        jar.signed_mut(&self.key).add(Cookie::new(
            self.config.cookie_name.clone(),
            token.to_string(),
        ));
        jar.get(&self.config.cookie_name)
            .expect("cookie was just added")
            .value()
            .to_string()
    }

    /// Verifies a signed cookie value and returns the plain token if valid.
    fn verify_token(&self, signed_value: &str) -> Option<String> {
        let mut jar = CookieJar::new();
        jar.add_original(Cookie::new(
            self.config.cookie_name.clone(),
            signed_value.to_string(),
        ));
        jar.signed(&self.key)
            .get(&self.config.cookie_name)
            .map(|c: Cookie<'_>| c.value().to_string())
    }

    /// Builds the Set-Cookie header value for the CSRF cookie.
    fn build_set_cookie(&self, signed_value: &str) -> String {
        Cookie::build((self.config.cookie_name.clone(), signed_value.to_string()))
            .http_only(true)
            .same_site(SameSite::Lax)
            .path("/")
            .max_age(cookie::time::Duration::seconds(self.config.ttl_secs as i64))
            .build()
            .to_string()
    }

    /// Returns `true` if the request method is exempt from CSRF checks.
    fn is_exempt(&self, method: &Method) -> bool {
        self.config
            .exempt_methods
            .iter()
            .any(|m| m.eq_ignore_ascii_case(method.as_str()))
    }

    /// Extracts the token submitted by the client from the header.
    fn extract_submitted_token<B>(&self, request: &Request<B>) -> Option<String> {
        request
            .headers()
            .get(&self.config.header_name)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string())
    }

    /// Extracts the cookie value from the request's Cookie header.
    fn extract_cookie_value<B>(&self, request: &Request<B>) -> Option<String> {
        let cookie_header = request.headers().get(http::header::COOKIE)?;
        let cookie_str = cookie_header.to_str().ok()?;

        for pair in cookie_str.split(';') {
            let pair = pair.trim();
            if let Some((name, value)) = pair.split_once('=')
                && name.trim() == self.config.cookie_name
            {
                return Some(value.trim().to_string());
            }
        }

        None
    }
}

impl<S, ReqBody> Service<Request<ReqBody>> for CsrfService<S>
where
    S: Service<Request<ReqBody>, Response = Response<Body>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Into<Box<dyn std::error::Error + Send + Sync>> + Send + 'static,
    ReqBody: Send + 'static,
{
    type Response = Response<Body>;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut request: Request<ReqBody>) -> Self::Future {
        // Clone self's inner service for use in the async block (tower pattern)
        let mut inner = self.inner.clone();
        std::mem::swap(&mut self.inner, &mut inner);

        let is_exempt = self.is_exempt(request.method());

        if is_exempt {
            // Generate a new token, sign it, set cookie, inject into extensions
            let token = crate::id::ulid();
            let signed_value = self.sign_token(&token);
            let set_cookie_value = self.build_set_cookie(&signed_value);

            request.extensions_mut().insert(CsrfToken(token.clone()));

            Box::pin(async move {
                let mut response = inner.call(request).await?;

                if let Ok(header_value) = HeaderValue::from_str(&set_cookie_value) {
                    response
                        .headers_mut()
                        .append(http::header::SET_COOKIE, header_value);
                }

                response.extensions_mut().insert(CsrfToken(token));

                Ok(response)
            })
        } else {
            // Validate: read signed cookie, verify, compare with submitted token
            let cookie_value = self.extract_cookie_value(&request);
            let submitted_token = self.extract_submitted_token(&request);

            let verified = cookie_value
                .and_then(|signed| self.verify_token(&signed))
                .zip(submitted_token)
                .is_some_and(|(cookie_token, header_token)| cookie_token == header_token);

            if verified {
                Box::pin(async move { inner.call(request).await })
            } else {
                let header_name = self.config.header_name.clone();
                Box::pin(async move {
                    let error = crate::error::Error::forbidden(format!(
                        "CSRF validation failed: missing or invalid {header_name}"
                    ));
                    let mut response =
                        (StatusCode::FORBIDDEN, error.message().to_string()).into_response();
                    response.extensions_mut().insert(error);
                    Ok(response)
                })
            }
        }
    }
}

/// Returns a Tower layer that applies CSRF protection using the
/// double-submit signed cookie pattern.
///
/// # Example
///
/// ```rust,no_run
/// use modo::middleware::{csrf, CsrfConfig};
/// use modo::cookie::Key;
///
/// let config = CsrfConfig::default();
/// let key = Key::generate();
/// let layer = csrf(&config, &key);
/// ```
pub fn csrf(config: &CsrfConfig, key: &Key) -> CsrfLayer {
    CsrfLayer {
        config: config.clone(),
        key: key.clone(),
    }
}
