use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use http::HeaderMap;
use http_body_util::Full;
use serde::Serialize;

use super::client::ClientInner;
use super::response::Response;
use super::retry::RetryPolicy;
use crate::error::{Error, Result};

/// A builder for constructing and sending an HTTP request.
///
/// Obtained from [`Client::get`](super::Client::get),
/// [`Client::post`](super::Client::post), etc. Chaining methods configure
/// headers, body, authentication, and retry behaviour. The terminal
/// [`send`](RequestBuilder::send) method dispatches the request.
///
/// # Examples
///
/// ```rust,ignore
/// let resp = client
///     .post("https://api.example.com/items")
///     .bearer_token("tok_abc")
///     .json(&payload)
///     .send()
///     .await?;
/// ```
pub struct RequestBuilder {
    inner: Arc<ClientInner>,
    method: http::Method,
    url: std::result::Result<http::Uri, Error>,
    headers: HeaderMap,
    body: Option<std::result::Result<Bytes, Error>>,
    deferred_error: Option<Error>,
    timeout: Option<Duration>,
    max_retries: Option<u32>,
}

impl RequestBuilder {
    /// Create a new request builder. Called internally by `Client`.
    pub(crate) fn new(inner: Arc<ClientInner>, method: http::Method, url: &str) -> Self {
        let url_result = url
            .parse::<http::Uri>()
            .map_err(|e| Error::bad_request(format!("invalid URL: {e}")).chain(e));
        Self {
            inner,
            method,
            url: url_result,
            headers: HeaderMap::new(),
            body: None,
            deferred_error: None,
            timeout: None,
            max_retries: None,
        }
    }

    /// Set a single header, replacing any existing value with the same name.
    pub fn header(
        mut self,
        name: http::header::HeaderName,
        value: http::header::HeaderValue,
    ) -> Self {
        self.headers.insert(name, value);
        self
    }

    /// Merge additional headers into the request.
    pub fn headers(mut self, headers: HeaderMap) -> Self {
        self.headers.extend(headers);
        self
    }

    /// Set a `Bearer` authorization header.
    pub fn bearer_token(mut self, token: impl AsRef<str>) -> Self {
        let value = format!("Bearer {}", token.as_ref());
        match http::header::HeaderValue::from_str(&value) {
            Ok(hv) => self.header(http::header::AUTHORIZATION, hv),
            Err(e) => {
                self.deferred_error = Some(
                    Error::bad_request(format!("invalid bearer token header value: {e}")).chain(e),
                );
                self
            }
        }
    }

    /// Set a `Basic` authorization header from username and password.
    pub fn basic_auth(mut self, username: &str, password: Option<&str>) -> Self {
        use base64::Engine;
        let credentials = match password {
            Some(pw) => format!("{username}:{pw}"),
            None => format!("{username}:"),
        };
        let encoded = base64::engine::general_purpose::STANDARD.encode(credentials);
        let value = format!("Basic {encoded}");
        match http::header::HeaderValue::from_str(&value) {
            Ok(hv) => self.header(http::header::AUTHORIZATION, hv),
            Err(e) => {
                self.deferred_error = Some(
                    Error::bad_request(format!("invalid basic auth header value: {e}")).chain(e),
                );
                self
            }
        }
    }

    /// Append query parameters to the URL.
    ///
    /// Parameters are URL-encoded and appended to any existing query string.
    pub fn query(mut self, params: &[(&str, &str)]) -> Self {
        self.url = self.url.and_then(|uri| {
            let encoded = params
                .iter()
                .map(|(k, v)| format!("{}={}", urlencoding(k), urlencoding(v)))
                .collect::<Vec<_>>()
                .join("&");

            if encoded.is_empty() {
                return Ok(uri);
            }

            let url = uri.to_string();
            let sep = if url.contains('?') { "&" } else { "?" };
            let new_url = format!("{url}{sep}{encoded}");
            new_url.parse::<http::Uri>().map_err(|e| {
                Error::bad_request(format!("invalid URL after appending query params: {e}"))
                    .chain(e)
            })
        });
        self
    }

    /// Set a JSON request body. Also sets `Content-Type: application/json`.
    pub fn json(mut self, value: &impl Serialize) -> Self {
        match serde_json::to_vec(value) {
            Ok(bytes) => {
                self.body = Some(Ok(Bytes::from(bytes)));
                self.headers.insert(
                    http::header::CONTENT_TYPE,
                    http::HeaderValue::from_static("application/json"),
                );
            }
            Err(e) => {
                self.body = Some(Err(Error::internal(format!(
                    "failed to serialize request body: {e}"
                ))
                .chain(e)));
            }
        }
        self
    }

    /// Set a URL-encoded form body. Also sets `Content-Type: application/x-www-form-urlencoded`.
    pub fn form<T: Serialize>(mut self, body: &T) -> Self {
        match serde_urlencoded::to_string(body) {
            Ok(encoded) => {
                self.body = Some(Ok(Bytes::from(encoded)));
                self.headers.insert(
                    http::header::CONTENT_TYPE,
                    http::HeaderValue::from_static("application/x-www-form-urlencoded"),
                );
            }
            Err(e) => {
                self.body = Some(Err(Error::internal(format!(
                    "failed to serialize request body: {e}"
                ))
                .chain(e)));
            }
        }
        self
    }

    /// Set a raw byte body.
    pub fn body(mut self, bytes: impl Into<Bytes>) -> Self {
        self.body = Some(Ok(bytes.into()));
        self
    }

    /// Override the request timeout for this request.
    pub fn timeout(mut self, duration: Duration) -> Self {
        self.timeout = Some(duration);
        self
    }

    /// Override the maximum retry count for this request.
    pub fn max_retries(mut self, n: u32) -> Self {
        self.max_retries = Some(n);
        self
    }

    /// Send the request and return the response.
    pub async fn send(self) -> Result<Response> {
        // Check deferred URL error first.
        let uri = self.url?;

        // Check deferred builder errors (auth header failures, etc.).
        if let Some(e) = self.deferred_error {
            return Err(e);
        }

        // Check deferred body error next.
        let body_bytes = match self.body {
            Some(Ok(b)) => b,
            Some(Err(e)) => return Err(e),
            None => Bytes::new(),
        };

        let inner = self.inner;
        let method = self.method;
        let mut headers = self.headers;
        let timeout = self.timeout.or_else(|| inner.config.timeout());
        let max_retries = self.max_retries.unwrap_or(inner.config.max_retries);

        // Apply default User-Agent only if not set by caller.
        if !headers.contains_key(http::header::USER_AGENT)
            && let Ok(ua) = http::header::HeaderValue::from_str(&inner.config.user_agent)
        {
            headers.insert(http::header::USER_AGENT, ua);
        }

        let url_string = uri.to_string();

        let policy = RetryPolicy {
            max_retries,
            backoff: inner.config.retry_backoff(),
        };

        let build_request = || {
            let mut builder = hyper::Request::builder()
                .method(method.clone())
                .uri(uri.clone());

            for (name, value) in &headers {
                builder = builder.header(name, value);
            }

            builder
                .body(Full::new(body_bytes.clone()))
                .map_err(|e| Error::internal(format!("failed to build request: {e}")).chain(e))
        };

        super::retry::execute(&inner.client, &policy, &url_string, timeout, build_request).await
    }
}

/// Percent-encode a string for use in URL query parameters.
fn urlencoding(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => {
                out.push('%');
                out.push(char::from(HEX[(b >> 4) as usize]));
                out.push(char::from(HEX[(b & 0x0F) as usize]));
            }
        }
    }
    out
}

/// Hex digits for percent-encoding.
const HEX: [u8; 16] = *b"0123456789ABCDEF";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urlencoding_plain() {
        assert_eq!(urlencoding("hello"), "hello");
    }

    #[test]
    fn urlencoding_special() {
        assert_eq!(urlencoding("a b"), "a%20b");
        assert_eq!(urlencoding("k=v&x"), "k%3Dv%26x");
    }

    #[test]
    fn urlencoding_preserves_unreserved() {
        assert_eq!(urlencoding("a-b_c.d~e"), "a-b_c.d~e");
    }
}
