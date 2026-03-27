# HTTP Client Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build an ergonomic async HTTP client module (`src/http/`) that consolidates three independent hyper client setups into one shared, cheaply-cloneable client.

**Architecture:** Thin wrapper over `hyper_util::client::legacy::Client` with `Arc<Inner>` pattern. `Client` → `RequestBuilder` → `Response` flow. Retry loop with exponential backoff and tracing. Feature-gated under `http-client`. After the module ships, webhook/storage/oauth modules are refactored to use it.

**Tech Stack:** hyper 1, hyper-rustls 0.27, hyper-util 0.1, http-body-util 0.1 (all already in dep tree)

**Spec:** `docs/superpowers/specs/2026-03-27-http-client-design.md`

---

### Task 1: Feature Flag and Module Skeleton

**Files:**
- Modify: `Cargo.toml` (feature definitions)
- Create: `src/http/mod.rs`
- Create: `src/http/config.rs`
- Modify: `src/lib.rs` (module declaration)
- Modify: `src/config/modo.rs` (Config field)

- [ ] **Step 1: Add `http-client` feature flag to `Cargo.toml`**

In `Cargo.toml`, add the `http-client` feature after line 22 (after `sse`), and update `auth`, `storage`, `webhooks` to depend on it instead of listing hyper deps directly. Also add `http-client` to the `full` feature.

```toml
http-client = ["dep:hyper", "dep:hyper-rustls", "dep:hyper-util", "dep:http-body-util"]
auth = [
  "http-client",
  "dep:argon2",
  "dep:hmac",
  "dep:sha1",
]
storage = ["http-client", "dep:hmac"]
webhooks = ["http-client", "dep:hmac", "dep:base64"]
```

Update the `full` feature to include `http-client`:
```toml
full = ["http-client", "templates", "sse", "auth", "sentry", "email", "storage", "webhooks", "dns", "geolocation", "qrcode"]
```

- [ ] **Step 2: Create `src/http/config.rs`**

```rust
use std::time::Duration;

use serde::Deserialize;

/// HTTP client configuration.
///
/// Deserializes from the `http:` section of the framework YAML config.
/// All fields have sensible defaults so the section can be omitted entirely.
#[non_exhaustive]
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ClientConfig {
    /// Default request timeout in seconds. `0` means no timeout.
    pub timeout_secs: u64,
    /// TCP connect timeout in seconds.
    pub connect_timeout_secs: u64,
    /// Default `User-Agent` header value.
    pub user_agent: String,
    /// Maximum retry attempts for retryable failures. `0` means no retries.
    pub max_retries: u32,
    /// Initial backoff between retries in milliseconds.
    /// Actual backoff is `retry_backoff_ms * 2^attempt`.
    pub retry_backoff_ms: u64,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            timeout_secs: 30,
            connect_timeout_secs: 5,
            user_agent: "modo/0.1".to_string(),
            max_retries: 0,
            retry_backoff_ms: 100,
        }
    }
}

impl ClientConfig {
    /// Request timeout as a `Duration`. Returns `None` when `timeout_secs` is `0`.
    pub(crate) fn timeout(&self) -> Option<Duration> {
        if self.timeout_secs == 0 {
            None
        } else {
            Some(Duration::from_secs(self.timeout_secs))
        }
    }

    /// TCP connect timeout as a `Duration`.
    pub(crate) fn connect_timeout(&self) -> Duration {
        Duration::from_secs(self.connect_timeout_secs)
    }

    /// Retry backoff base as a `Duration`.
    pub(crate) fn retry_backoff(&self) -> Duration {
        Duration::from_millis(self.retry_backoff_ms)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_values() {
        let config = ClientConfig::default();
        assert_eq!(config.timeout_secs, 30);
        assert_eq!(config.connect_timeout_secs, 5);
        assert_eq!(config.user_agent, "modo/0.1");
        assert_eq!(config.max_retries, 0);
        assert_eq!(config.retry_backoff_ms, 100);
    }

    #[test]
    fn timeout_returns_none_for_zero() {
        let config = ClientConfig {
            timeout_secs: 0,
            ..Default::default()
        };
        assert!(config.timeout().is_none());
    }

    #[test]
    fn timeout_returns_duration_for_nonzero() {
        let config = ClientConfig::default();
        assert_eq!(config.timeout(), Some(Duration::from_secs(30)));
    }

    #[test]
    fn deserialize_from_yaml() {
        let yaml = r#"
timeout_secs: 10
connect_timeout_secs: 2
user_agent: "myapp/1.0"
max_retries: 3
retry_backoff_ms: 200
"#;
        let config: ClientConfig = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(config.timeout_secs, 10);
        assert_eq!(config.connect_timeout_secs, 2);
        assert_eq!(config.user_agent, "myapp/1.0");
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.retry_backoff_ms, 200);
    }

    #[test]
    fn deserialize_empty_yaml_uses_defaults() {
        let config: ClientConfig = serde_yaml_ng::from_str("{}").unwrap();
        assert_eq!(config.timeout_secs, 30);
        assert_eq!(config.max_retries, 0);
    }
}
```

- [ ] **Step 3: Create `src/http/mod.rs`**

```rust
mod config;

pub use config::ClientConfig;
```

- [ ] **Step 4: Add module declaration to `src/lib.rs`**

After line 50 (after `pub mod tenant;`), add:

```rust
#[cfg(feature = "http-client")]
pub mod http;
```

- [ ] **Step 5: Add config field to `src/config/modo.rs`**

After the `pub session` field (line 34), add:

```rust
    /// HTTP client settings (timeout, retries, user agent).
    /// Requires the `http-client` feature.
    #[cfg(feature = "http-client")]
    #[serde(default)]
    pub http: crate::http::ClientConfig,
```

- [ ] **Step 6: Add re-export to `src/lib.rs`**

After line 82 (`pub use error::{Error, Result};`), add:

```rust
#[cfg(feature = "http-client")]
pub use http::ClientConfig as HttpClientConfig;
```

- [ ] **Step 7: Run tests and lint**

Run: `cargo test --features http-client`
Expected: All tests pass including the new config tests.

Run: `cargo clippy --features http-client --tests -- -D warnings`
Expected: No warnings.

- [ ] **Step 8: Commit**

```bash
git add Cargo.toml src/http/ src/lib.rs src/config/modo.rs
git commit -m "feat(http): add http-client feature flag and ClientConfig"
```

---

### Task 2: Response Type

**Files:**
- Create: `src/http/response.rs`
- Modify: `src/http/mod.rs`

- [ ] **Step 1: Create `src/http/response.rs`**

```rust
use bytes::Bytes;
use http::{HeaderMap, StatusCode};
use http_body_util::BodyExt;
use serde::de::DeserializeOwned;

use crate::error::{Error, Result};

/// HTTP response returned by [`Client::send`](super::Client).
///
/// Body consumption methods (`json`, `text`, `bytes`) take ownership of `self`
/// — the body can only be read once. For streaming large responses use
/// [`stream`](Response::stream).
pub struct Response {
    status: StatusCode,
    headers: HeaderMap,
    url: String,
    body: hyper::body::Incoming,
}

impl Response {
    pub(crate) fn new(
        status: StatusCode,
        headers: HeaderMap,
        url: String,
        body: hyper::body::Incoming,
    ) -> Self {
        Self {
            status,
            headers,
            url,
            body,
        }
    }

    /// HTTP status code of the response.
    pub fn status(&self) -> StatusCode {
        self.status
    }

    /// Response headers.
    pub fn headers(&self) -> &HeaderMap {
        &self.headers
    }

    /// The URL that was requested (after query-param appending, before redirects).
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Value of the `Content-Length` header, if present and parseable.
    pub fn content_length(&self) -> Option<u64> {
        self.headers
            .get(http::header::CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse().ok())
    }

    /// Read the full body and deserialize as JSON.
    pub async fn json<T: DeserializeOwned>(self) -> Result<T> {
        let bytes = self.bytes().await?;
        serde_json::from_slice(&bytes)
            .map_err(|e| Error::internal(format!("failed to parse response JSON: {e}")).chain(e))
    }

    /// Read the full body as a UTF-8 string.
    pub async fn text(self) -> Result<String> {
        let bytes = self.bytes().await?;
        String::from_utf8(bytes.into())
            .map_err(|e| Error::internal("response body is not valid UTF-8").chain(e))
    }

    /// Read the full body as raw bytes.
    pub async fn bytes(self) -> Result<Bytes> {
        self.body
            .collect()
            .await
            .map(|collected| collected.to_bytes())
            .map_err(|e| Error::internal(format!("failed to read response body: {e}")).chain(e))
    }

    /// Return a stream of body chunks without buffering the full response.
    pub fn stream(self) -> BodyStream {
        BodyStream {
            body: self.body,
        }
    }

    /// Return `Ok(self)` if the status is 2xx, or `Err` otherwise.
    ///
    /// For fine-grained status handling, check [`status`](Response::status)
    /// directly and build your own error.
    pub fn error_for_status(self) -> Result<Self> {
        if self.status.is_success() {
            Ok(self)
        } else {
            Err(Error::internal(format!("HTTP {}: {}", self.status, self.url)))
        }
    }
}

/// Streaming body wrapper returned by [`Response::stream`].
pub struct BodyStream {
    body: hyper::body::Incoming,
}

impl BodyStream {
    /// Yield the next chunk of body bytes.
    ///
    /// Returns `None` when the body is fully consumed.
    pub async fn next(&mut self) -> Option<Result<Bytes>> {
        use hyper::body::Frame;

        loop {
            let frame = std::pin::pin!(self.body.frame()).await?;
            match frame {
                Ok(frame) => {
                    if let Some(data) = frame.into_data().ok() {
                        return Some(Ok(data));
                    }
                    // Skip non-data frames (trailers) and continue
                }
                Err(e) => {
                    return Some(Err(
                        Error::internal(format!("failed to read response body: {e}")).chain(e),
                    ));
                }
            }
        }
    }
}
```

- [ ] **Step 2: Update `src/http/mod.rs`**

```rust
mod config;
mod response;

pub use config::ClientConfig;
pub use response::{BodyStream, Response};
```

- [ ] **Step 3: Run tests and lint**

Run: `cargo check --features http-client`
Expected: Compiles without errors.

Run: `cargo clippy --features http-client --tests -- -D warnings`
Expected: No warnings.

- [ ] **Step 4: Commit**

```bash
git add src/http/response.rs src/http/mod.rs
git commit -m "feat(http): add Response and BodyStream types"
```

---

### Task 3: Retry Policy

**Files:**
- Create: `src/http/retry.rs`
- Modify: `src/http/mod.rs`

- [ ] **Step 1: Create `src/http/retry.rs`**

```rust
use std::time::Duration;

use bytes::Bytes;
use http::{Method, StatusCode, Uri};
use http::header::HeaderMap;
use http_body_util::Full;
use hyper_rustls::HttpsConnector;
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;

use super::response::Response;
use crate::error::{Error, Result};

/// Max duration we'll honour for a `Retry-After` header value.
const MAX_RETRY_AFTER: Duration = Duration::from_secs(60);

pub(crate) struct RetryPolicy {
    pub max_retries: u32,
    pub backoff: Duration,
}

/// Outcome of a single request attempt.
enum Attempt {
    /// A complete HTTP response was received.
    Success(Response),
    /// The attempt failed with a retryable condition.
    /// Optional duration is the server-requested Retry-After delay.
    Retryable(String, Option<Duration>),
    /// The attempt failed with a non-retryable error.
    Fatal(Error),
}

pub(crate) async fn execute(
    client: &Client<HttpsConnector<HttpConnector>, Full<Bytes>>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Option<Bytes>,
    timeout: Option<Duration>,
    policy: &RetryPolicy,
) -> Result<Response> {
    let url_str = uri.to_string();
    let method_str = method.as_str();
    let max = policy.max_retries;

    let mut last_error: Option<Error> = None;

    for attempt in 0..=max {
        tracing::debug!(
            attempt,
            url = %url_str,
            method = %method_str,
            "http.request",
        );

        let result = send_once(client, &method, &uri, &headers, body.as_ref(), timeout).await;

        match classify(result, &url_str) {
            Attempt::Success(resp) => return Ok(resp),
            Attempt::Fatal(err) => return Err(err),
            Attempt::Retryable(reason, retry_after) => {
                if attempt < max {
                    // Use Retry-After from server if provided, otherwise exponential backoff
                    let delay = retry_after.unwrap_or_else(|| backoff_delay(policy.backoff, attempt));
                    tracing::warn!(
                        attempt,
                        next_attempt = attempt + 1,
                        reason = %reason,
                        backoff_ms = delay.as_millis() as u64,
                        "http.retry",
                    );
                    tokio::time::sleep(delay).await;
                } else {
                    last_error = Some(Error::internal(format!(
                        "HTTP request failed after {max} retries: {reason}"
                    )));
                }
            }
        }
    }

    Err(last_error.unwrap_or_else(|| Error::internal("HTTP request failed")))
}

async fn send_once(
    client: &Client<HttpsConnector<HttpConnector>, Full<Bytes>>,
    method: &Method,
    uri: &Uri,
    headers: &HeaderMap,
    body: Option<&Bytes>,
    timeout: Option<Duration>,
) -> std::result::Result<hyper::Response<hyper::body::Incoming>, SendError> {
    let req_body = match body {
        Some(b) => Full::new(b.clone()),
        None => Full::new(Bytes::new()),
    };

    let mut builder = hyper::Request::builder().method(method).uri(uri);
    for (name, value) in headers {
        builder = builder.header(name, value);
    }

    let request = builder.body(req_body).map_err(|e| SendError::Fatal(e.to_string()))?;

    let fut = client.request(request);
    match timeout {
        Some(dur) => tokio::time::timeout(dur, fut)
            .await
            .map_err(|_| SendError::Timeout)?
            .map_err(|e| SendError::Connection(e.to_string())),
        None => fut
            .await
            .map_err(|e| SendError::Connection(e.to_string())),
    }
}

enum SendError {
    Timeout,
    Connection(String),
    Fatal(String),
}

fn classify(
    result: std::result::Result<hyper::Response<hyper::body::Incoming>, SendError>,
    url: &str,
) -> Attempt {
    match result {
        Ok(resp) => {
            let status = resp.status();
            match status {
                s if s == StatusCode::BAD_GATEWAY || s == StatusCode::SERVICE_UNAVAILABLE => {
                    Attempt::Retryable(format!("HTTP {s}"), None)
                }
                StatusCode::TOO_MANY_REQUESTS => {
                    let retry_after = parse_retry_after(resp.headers());
                    Attempt::Retryable("HTTP 429".to_string(), retry_after)
                }
                _ => {
                    let headers = resp.headers().clone();
                    let body = resp.into_body();
                    Attempt::Success(Response::new(status, headers, url.to_string(), body))
                }
            }
        }
        Err(SendError::Timeout) => Attempt::Retryable("timeout".to_string(), None),
        Err(SendError::Connection(msg)) => {
            Attempt::Retryable(format!("connection error: {msg}"), None)
        }
        Err(SendError::Fatal(msg)) => {
            Attempt::Fatal(Error::internal(format!("failed to build HTTP request: {msg}")))
        }
    }
}

/// Parse the `Retry-After` header as a number of seconds.
/// Ignores HTTP-date format (uncommon for APIs). Caps at 60 seconds.
fn parse_retry_after(headers: &HeaderMap) -> Option<Duration> {
    let value = headers.get("retry-after")?.to_str().ok()?;
    let secs: u64 = value.trim().parse().ok()?;
    Some(Duration::from_secs(secs).min(MAX_RETRY_AFTER))
}

fn backoff_delay(base: Duration, attempt: u32) -> Duration {
    base.saturating_mul(2u32.saturating_pow(attempt))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_delay_exponential() {
        let base = Duration::from_millis(100);
        assert_eq!(backoff_delay(base, 0), Duration::from_millis(100));
        assert_eq!(backoff_delay(base, 1), Duration::from_millis(200));
        assert_eq!(backoff_delay(base, 2), Duration::from_millis(400));
        assert_eq!(backoff_delay(base, 3), Duration::from_millis(800));
    }

    #[test]
    fn backoff_delay_does_not_overflow() {
        let base = Duration::from_millis(100);
        // Very high attempt should saturate, not panic
        let delay = backoff_delay(base, 100);
        assert!(delay >= base);
    }

    #[test]
    fn parse_retry_after_valid_seconds() {
        let mut headers = HeaderMap::new();
        headers.insert("retry-after", "5".parse().unwrap());
        assert_eq!(parse_retry_after(&headers), Some(Duration::from_secs(5)));
    }

    #[test]
    fn parse_retry_after_capped_at_60() {
        let mut headers = HeaderMap::new();
        headers.insert("retry-after", "120".parse().unwrap());
        assert_eq!(parse_retry_after(&headers), Some(Duration::from_secs(60)));
    }

    #[test]
    fn parse_retry_after_missing_header() {
        let headers = HeaderMap::new();
        assert_eq!(parse_retry_after(&headers), None);
    }

    #[test]
    fn parse_retry_after_non_numeric() {
        let mut headers = HeaderMap::new();
        headers.insert("retry-after", "Wed, 21 Oct 2026 07:28:00 GMT".parse().unwrap());
        // HTTP-date format is not supported — returns None
        assert_eq!(parse_retry_after(&headers), None);
    }
}
```

- [ ] **Step 2: Update `src/http/mod.rs`**

```rust
mod config;
mod response;
mod retry;

pub use config::ClientConfig;
pub use response::{BodyStream, Response};
```

- [ ] **Step 3: Run tests and lint**

Run: `cargo test --features http-client`
Expected: All tests pass including backoff unit tests.

Run: `cargo clippy --features http-client --tests -- -D warnings`
Expected: No warnings.

- [ ] **Step 4: Commit**

```bash
git add src/http/retry.rs src/http/mod.rs
git commit -m "feat(http): add retry policy with exponential backoff"
```

---

### Task 4: Client and RequestBuilder

**Files:**
- Create: `src/http/client.rs`
- Create: `src/http/request.rs`
- Modify: `src/http/mod.rs`
- Modify: `src/lib.rs` (re-exports)

- [ ] **Step 1: Create `src/http/request.rs`**

```rust
use std::time::Duration;

use bytes::Bytes;
use http::header::{HeaderMap, HeaderName, HeaderValue, AUTHORIZATION, CONTENT_TYPE, USER_AGENT};
use http::{Method, Uri};

use super::client::ClientInner;
use super::response::Response;
use super::retry::RetryPolicy;
use crate::error::{Error, Result};

use std::sync::Arc;

/// Builder for an outgoing HTTP request.
///
/// Created by [`Client::get`](super::Client::get),
/// [`Client::post`](super::Client::post), etc. Chain methods to add headers,
/// body, and per-request overrides, then call [`send`](RequestBuilder::send).
pub struct RequestBuilder {
    pub(crate) client: Arc<ClientInner>,
    pub(crate) method: Method,
    pub(crate) url: Result<Uri>,
    pub(crate) headers: HeaderMap,
    pub(crate) body: Option<std::result::Result<Bytes, Error>>,
    pub(crate) timeout: Option<Duration>,
    pub(crate) max_retries: Option<u32>,
}

impl RequestBuilder {
    /// Add a single header.
    pub fn header(mut self, name: HeaderName, value: HeaderValue) -> Self {
        self.headers.insert(name, value);
        self
    }

    /// Merge multiple headers into the request.
    pub fn headers(mut self, headers: HeaderMap) -> Self {
        self.headers.extend(headers);
        self
    }

    /// Set the `Authorization: Bearer {token}` header.
    pub fn bearer_token(self, token: &str) -> Self {
        let value = format!("Bearer {token}");
        // HeaderValue::from_str can fail on non-visible ASCII.
        // Tokens should always be valid header values.
        let hv = match HeaderValue::from_str(&value) {
            Ok(v) => v,
            Err(_) => return self,
        };
        self.header(AUTHORIZATION, hv)
    }

    /// Set the `Authorization: Basic {credentials}` header.
    ///
    /// `password` is optional — some APIs use username-only basic auth.
    pub fn basic_auth(self, username: &str, password: Option<&str>) -> Self {
        use base64::Engine;
        let credentials = match password {
            Some(pw) => format!("{username}:{pw}"),
            None => format!("{username}:"),
        };
        let encoded = base64::engine::general_purpose::STANDARD.encode(credentials);
        let value = format!("Basic {encoded}");
        let hv = match HeaderValue::from_str(&value) {
            Ok(v) => v,
            Err(_) => return self,
        };
        self.header(AUTHORIZATION, hv)
    }

    /// Append query parameters to the URL.
    ///
    /// Existing query parameters are preserved. Multiple calls accumulate.
    pub fn query(mut self, params: &[(&str, &str)]) -> Self {
        self.url = self.url.map(|uri| {
            let mut parts = uri.into_parts();
            let existing = parts
                .path_and_query
                .as_ref()
                .and_then(|pq| pq.query())
                .unwrap_or("");
            let path = parts
                .path_and_query
                .as_ref()
                .map(|pq| pq.path())
                .unwrap_or("/");

            let new_params: String = params
                .iter()
                .map(|(k, v)| format!("{}={}", urlencoding(k), urlencoding(v)))
                .collect::<Vec<_>>()
                .join("&");

            let query_string = if existing.is_empty() {
                new_params
            } else {
                format!("{existing}&{new_params}")
            };

            let pq = if query_string.is_empty() {
                path.to_string()
            } else {
                format!("{path}?{query_string}")
            };

            parts.path_and_query = Some(pq.parse().unwrap_or_else(|_| "/".parse().unwrap()));
            Uri::from_parts(parts).unwrap_or_else(|_| Uri::from_static("/"))
        });
        self
    }

    /// Serialize `body` as JSON and set `Content-Type: application/json`.
    pub fn json<T: serde::Serialize>(mut self, body: &T) -> Self {
        match serde_json::to_vec(body) {
            Ok(bytes) => {
                self.body = Some(Ok(Bytes::from(bytes)));
                self.headers
                    .insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
            }
            Err(e) => {
                self.body = Some(Err(
                    Error::internal(format!("failed to serialize request body: {e}")).chain(e),
                ));
            }
        }
        self
    }

    /// URL-encode `body` as form data and set
    /// `Content-Type: application/x-www-form-urlencoded`.
    pub fn form<T: serde::Serialize>(mut self, body: &T) -> Self {
        match serde_urlencoded::to_string(body) {
            Ok(encoded) => {
                self.body = Some(Ok(Bytes::from(encoded)));
                self.headers.insert(
                    CONTENT_TYPE,
                    HeaderValue::from_static("application/x-www-form-urlencoded"),
                );
            }
            Err(e) => {
                self.body = Some(Err(
                    Error::internal(format!("failed to serialize request body: {e}")).chain(e),
                ));
            }
        }
        self
    }

    /// Set a raw body.
    pub fn body(mut self, bytes: Bytes) -> Self {
        self.body = Some(Ok(bytes));
        self
    }

    /// Override the client-level request timeout for this request.
    pub fn timeout(mut self, duration: Duration) -> Self {
        self.timeout = Some(duration);
        self
    }

    /// Override the client-level retry count for this request.
    pub fn max_retries(mut self, n: u32) -> Self {
        self.max_retries = Some(n);
        self
    }

    /// Send the request and return the response.
    ///
    /// Returns `Ok(Response)` for any completed HTTP exchange (including 4xx/5xx).
    /// Returns `Err` only for connection failures, timeouts, and serialization errors.
    pub async fn send(self) -> Result<Response> {
        // Check deferred URL error
        let uri = self.url?;

        // Check deferred body serialization error
        let body = match self.body {
            Some(Ok(b)) => Some(b),
            Some(Err(e)) => return Err(e),
            None => None,
        };

        // Apply default User-Agent if not set by caller
        let mut headers = self.headers;
        if !headers.contains_key(USER_AGENT) {
            if let Ok(ua) = HeaderValue::from_str(&self.client.config.user_agent) {
                headers.insert(USER_AGENT, ua);
            }
        }

        let timeout = self.timeout.or(self.client.config.timeout());
        let max_retries = self.max_retries.unwrap_or(self.client.config.max_retries);

        let policy = RetryPolicy {
            max_retries,
            backoff: self.client.config.retry_backoff(),
        };

        super::retry::execute(
            &self.client.client,
            self.method,
            uri,
            headers,
            body,
            timeout,
            &policy,
        )
        .await
    }
}

/// Percent-encode a string for use in query parameters.
fn urlencoding(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                result.push(b as char);
            }
            _ => {
                result.push_str(&format!("%{b:02X}"));
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urlencoding_leaves_unreserved_intact() {
        assert_eq!(urlencoding("hello"), "hello");
        assert_eq!(urlencoding("a-b_c.d~e"), "a-b_c.d~e");
    }

    #[test]
    fn urlencoding_encodes_special_chars() {
        assert_eq!(urlencoding("a b"), "a%20b");
        assert_eq!(urlencoding("a&b=c"), "a%26b%3Dc");
    }
}
```

- [ ] **Step 2: Create `src/http/client.rs`**

```rust
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use http::Method;
use http::header::HeaderMap;
use http_body_util::Full;
use hyper_rustls::HttpsConnectorBuilder;
use hyper_util::client::legacy::Client as HyperClient;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::TokioExecutor;

use super::config::ClientConfig;
use super::request::RequestBuilder;

pub(crate) struct ClientInner {
    pub(crate) client: HyperClient<
        hyper_rustls::HttpsConnector<HttpConnector>,
        Full<Bytes>,
    >,
    pub(crate) config: ClientConfig,
}

/// Async HTTP client.
///
/// Cheaply cloneable (`Arc<Inner>` internally). All clones share one
/// connection pool. Create with [`Client::new`] from a [`ClientConfig`] or
/// use [`Client::default`] / [`Client::builder`].
pub struct Client {
    inner: Arc<ClientInner>,
}

impl Clone for Client {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl Client {
    /// Create a client from a [`ClientConfig`].
    ///
    /// This is the primary constructor — [`Client::default`] and
    /// [`ClientBuilder::build`] both funnel through it.
    pub fn new(config: &ClientConfig) -> Self {
        let mut http_connector = HttpConnector::new();
        http_connector.set_connect_timeout(Some(config.connect_timeout()));

        let connector = HttpsConnectorBuilder::new()
            .with_webpki_roots()
            .https_or_http()
            .enable_http1()
            .wrap_connector(http_connector);

        let client = HyperClient::builder(TokioExecutor::new()).build(connector);

        Self {
            inner: Arc::new(ClientInner {
                client,
                config: config.clone(),
            }),
        }
    }

    /// Create a client with default configuration.
    pub fn default() -> Self {
        Self::new(&ClientConfig::default())
    }

    /// Start building a client with fine-grained settings.
    pub fn builder() -> ClientBuilder {
        ClientBuilder {
            config: ClientConfig::default(),
        }
    }

    /// Start a GET request.
    pub fn get(&self, url: &str) -> RequestBuilder {
        self.request(Method::GET, url)
    }

    /// Start a POST request.
    pub fn post(&self, url: &str) -> RequestBuilder {
        self.request(Method::POST, url)
    }

    /// Start a PUT request.
    pub fn put(&self, url: &str) -> RequestBuilder {
        self.request(Method::PUT, url)
    }

    /// Start a PATCH request.
    pub fn patch(&self, url: &str) -> RequestBuilder {
        self.request(Method::PATCH, url)
    }

    /// Start a DELETE request.
    pub fn delete(&self, url: &str) -> RequestBuilder {
        self.request(Method::DELETE, url)
    }

    /// Start a request with an arbitrary HTTP method.
    pub fn request(&self, method: Method, url: &str) -> RequestBuilder {
        let url_result = url
            .parse()
            .map_err(|e| crate::Error::bad_request(format!("invalid URL: {e}")));
        RequestBuilder {
            client: Arc::clone(&self.inner),
            method,
            url: url_result,
            headers: HeaderMap::new(),
            body: None,
            timeout: None,
            max_retries: None,
        }
    }
}

/// Builder for constructing a [`Client`] with programmatic settings.
pub struct ClientBuilder {
    config: ClientConfig,
}

impl ClientBuilder {
    /// Set the request timeout.
    pub fn timeout(mut self, d: Duration) -> Self {
        self.config.timeout_secs = d.as_secs();
        self
    }

    /// Set the TCP connect timeout.
    pub fn connect_timeout(mut self, d: Duration) -> Self {
        self.config.connect_timeout_secs = d.as_secs();
        self
    }

    /// Set the default User-Agent header.
    pub fn user_agent(mut self, ua: impl Into<String>) -> Self {
        self.config.user_agent = ua.into();
        self
    }

    /// Set the maximum number of retries for retryable failures.
    pub fn max_retries(mut self, n: u32) -> Self {
        self.config.max_retries = n;
        self
    }

    /// Set the initial backoff duration between retries.
    pub fn retry_backoff(mut self, d: Duration) -> Self {
        self.config.retry_backoff_ms = d.as_millis() as u64;
        self
    }

    /// Build the [`Client`].
    pub fn build(self) -> Client {
        Client::new(&self.config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_default_creates_without_panic() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let _ = Client::default();
    }

    #[test]
    fn client_from_config_creates_without_panic() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let _ = Client::new(&ClientConfig::default());
    }

    #[test]
    fn client_builder_creates_without_panic() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let _ = Client::builder()
            .timeout(Duration::from_secs(10))
            .connect_timeout(Duration::from_secs(2))
            .user_agent("test/1.0")
            .max_retries(3)
            .retry_backoff(Duration::from_millis(200))
            .build();
    }

    #[test]
    fn client_is_clone_cheap() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let client = Client::default();
        let _ = client.clone();
    }
}
```

- [ ] **Step 3: Update `src/http/mod.rs`**

```rust
mod client;
mod config;
mod request;
mod response;
mod retry;

pub use client::{Client, ClientBuilder};
pub use config::ClientConfig;
pub use request::RequestBuilder;
pub use response::{BodyStream, Response};
```

- [ ] **Step 4: Update re-exports in `src/lib.rs`**

Replace the existing `http-client` re-export line with:

```rust
#[cfg(feature = "http-client")]
pub use http::{Client as HttpClient, ClientBuilder as HttpClientBuilder, ClientConfig as HttpClientConfig};
```

- [ ] **Step 5: Add `base64` dependency to `http-client` feature**

In `Cargo.toml`, update the `http-client` feature to include `dep:base64`:

```toml
http-client = ["dep:hyper", "dep:hyper-rustls", "dep:hyper-util", "dep:http-body-util", "dep:base64"]
```

And remove `dep:base64` from `webhooks` since it now gets it transitively via `http-client`:

```toml
webhooks = ["http-client", "dep:hmac"]
```

- [ ] **Step 6: Run tests and lint**

Run: `cargo test --features http-client`
Expected: All tests pass.

Run: `cargo clippy --features http-client --tests -- -D warnings`
Expected: No warnings.

Also verify existing features still compile:
Run: `cargo check --features auth`
Run: `cargo check --features storage`
Run: `cargo check --features webhooks`

- [ ] **Step 7: Commit**

```bash
git add src/http/ src/lib.rs Cargo.toml
git commit -m "feat(http): add Client, RequestBuilder, and builder API"
```

---

### Task 5: Integration Tests

**Files:**
- Create: `tests/http_client.rs`

- [ ] **Step 1: Create `tests/http_client.rs`**

```rust
#![cfg(feature = "http-client")]

use std::time::Duration;

use modo::http::{Client, ClientConfig};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// Start a test server that accepts one connection and sends a canned response.
async fn start_server(
    response: &'static str,
) -> (String, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let url = format!("http://127.0.0.1:{}", addr.port());

    let handle = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut buf = vec![0u8; 4096];
        let _ = stream.read(&mut buf).await.unwrap();
        stream.write_all(response.as_bytes()).await.unwrap();
        stream.shutdown().await.unwrap();
    });

    (url, handle)
}

/// Start a test server that handles multiple sequential connections.
async fn start_multi_server(
    responses: Vec<&'static str>,
) -> (String, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let url = format!("http://127.0.0.1:{}", addr.port());

    let handle = tokio::spawn(async move {
        for response in responses {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 4096];
            let _ = stream.read(&mut buf).await.unwrap();
            stream.write_all(response.as_bytes()).await.unwrap();
            stream.shutdown().await.unwrap();
        }
    });

    (url, handle)
}

fn test_client() -> Client {
    let _ = rustls::crypto::ring::default_provider().install_default();
    Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
}

#[tokio::test]
async fn get_json_round_trip() {
    let body = r#"{"name":"modo","version":1}"#;
    let response_str = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body,
    );
    // Leak to get 'static lifetime for the test server
    let response_static: &'static str = Box::leak(response_str.into_boxed_str());
    let (url, handle) = start_server(response_static).await;
    let client = test_client();

    #[derive(serde::Deserialize)]
    struct Info {
        name: String,
        version: u32,
    }

    let resp = client.get(&url).send().await.unwrap();
    assert_eq!(resp.status(), http::StatusCode::OK);

    let info: Info = resp.json().await.unwrap();
    assert_eq!(info.name, "modo");
    assert_eq!(info.version, 1);

    handle.await.unwrap();
}

#[tokio::test]
async fn post_json_body() {
    let response_str = "HTTP/1.1 201 Created\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
    let (url, handle) = start_server(response_str).await;
    let client = test_client();

    #[derive(serde::Serialize)]
    struct Payload {
        key: String,
    }

    let resp = client
        .post(&url)
        .json(&Payload {
            key: "value".into(),
        })
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), http::StatusCode::CREATED);

    handle.await.unwrap();
}

#[tokio::test]
async fn timeout_returns_error() {
    // Server that never responds
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let url = format!("http://127.0.0.1:{}", addr.port());

    let _handle = tokio::spawn(async move {
        let (mut _stream, _) = listener.accept().await.unwrap();
        // Hold the connection open but never respond
        tokio::time::sleep(Duration::from_secs(60)).await;
    });

    let client = Client::builder()
        .timeout(Duration::from_millis(100))
        .build();

    let result = client.get(&url).send().await;
    assert!(result.is_err());
    let err = result.err().unwrap();
    assert!(
        err.message().contains("timeout") || err.message().contains("retries"),
        "unexpected error: {}",
        err.message()
    );
}

#[tokio::test]
async fn connection_refused_returns_error() {
    // Find a port nothing is listening on
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    let url = format!("http://127.0.0.1:{port}");
    let client = test_client();

    let result = client.get(&url).send().await;
    assert!(result.is_err());
}

#[tokio::test]
async fn error_for_status_on_4xx() {
    let (url, handle) = start_server(
        "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
    )
    .await;
    let client = test_client();

    let resp = client.get(&url).send().await.unwrap();
    assert_eq!(resp.status(), http::StatusCode::NOT_FOUND);

    // error_for_status should convert to Err
    let (url2, handle2) = start_server(
        "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
    )
    .await;
    let result = client.get(&url2).send().await.unwrap().error_for_status();
    assert!(result.is_err());

    handle.await.unwrap();
    handle2.await.unwrap();
}

#[tokio::test]
async fn error_for_status_passes_2xx() {
    let (url, handle) = start_server(
        "HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
    )
    .await;
    let client = test_client();

    let result = client.get(&url).send().await.unwrap().error_for_status();
    assert!(result.is_ok());

    handle.await.unwrap();
}

#[tokio::test]
async fn retry_on_503() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    let responses = vec![
        "HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok",
    ];
    let (url, handle) = start_multi_server(responses).await;

    let client = Client::builder()
        .timeout(Duration::from_secs(5))
        .max_retries(2)
        .retry_backoff(Duration::from_millis(10))
        .build();

    let resp = client.get(&url).send().await.unwrap();
    assert_eq!(resp.status(), http::StatusCode::OK);
    let body = resp.text().await.unwrap();
    assert_eq!(body, "ok");

    handle.await.unwrap();
}

#[tokio::test]
async fn text_response() {
    let body = "hello world";
    let response_str = format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body,
    );
    let response_static: &'static str = Box::leak(response_str.into_boxed_str());
    let (url, handle) = start_server(response_static).await;
    let client = test_client();

    let text = client.get(&url).send().await.unwrap().text().await.unwrap();
    assert_eq!(text, "hello world");

    handle.await.unwrap();
}

#[tokio::test]
async fn bytes_response() {
    let body = b"binary\x00data";
    let response_str = format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len(),
    );
    // For binary data, build the response manually
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let url = format!("http://127.0.0.1:{}", addr.port());

    let handle = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut buf = vec![0u8; 4096];
        let _ = stream.read(&mut buf).await.unwrap();
        stream.write_all(response_str.as_bytes()).await.unwrap();
        stream.write_all(body).await.unwrap();
        stream.shutdown().await.unwrap();
    });

    let client = test_client();
    let data = client.get(&url).send().await.unwrap().bytes().await.unwrap();
    assert_eq!(&data[..], body);

    handle.await.unwrap();
}

#[tokio::test]
async fn stream_yields_chunks() {
    let body = "chunk data here";
    let response_str = format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body,
    );
    let response_static: &'static str = Box::leak(response_str.into_boxed_str());
    let (url, handle) = start_server(response_static).await;
    let client = test_client();

    let resp = client.get(&url).send().await.unwrap();
    let mut stream = resp.stream();
    let mut collected = Vec::new();
    while let Some(chunk) = stream.next().await {
        collected.extend_from_slice(&chunk.unwrap());
    }
    assert_eq!(String::from_utf8(collected).unwrap(), "chunk data here");

    handle.await.unwrap();
}

#[tokio::test]
async fn invalid_url_returns_error() {
    let client = test_client();
    let result = client.get("not a valid url").send().await;
    assert!(result.is_err());
}

#[tokio::test]
async fn bearer_token_header() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let url = format!("http://127.0.0.1:{}", addr.port());

    let handle = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut buf = vec![0u8; 4096];
        let n = stream.read(&mut buf).await.unwrap();
        let request_text = String::from_utf8_lossy(&buf[..n]);

        // Verify the Authorization header was sent
        assert!(
            request_text.contains("authorization: Bearer test-token"),
            "missing bearer token in: {request_text}"
        );

        let resp = "HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
        stream.write_all(resp.as_bytes()).await.unwrap();
        stream.shutdown().await.unwrap();
    });

    let client = test_client();
    let resp = client
        .get(&url)
        .bearer_token("test-token")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), http::StatusCode::OK);

    handle.await.unwrap();
}

#[tokio::test]
async fn query_params_appended() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let url = format!("http://127.0.0.1:{}/path", addr.port());

    let handle = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut buf = vec![0u8; 4096];
        let n = stream.read(&mut buf).await.unwrap();
        let request_text = String::from_utf8_lossy(&buf[..n]);

        assert!(
            request_text.contains("/path?foo=bar&baz=qux"),
            "unexpected request line: {request_text}"
        );

        let resp = "HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
        stream.write_all(resp.as_bytes()).await.unwrap();
        stream.shutdown().await.unwrap();
    });

    let client = test_client();
    let resp = client
        .get(&url)
        .query(&[("foo", "bar"), ("baz", "qux")])
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), http::StatusCode::OK);

    handle.await.unwrap();
}

#[tokio::test]
async fn content_length_from_headers() {
    let (url, handle) = start_server(
        "HTTP/1.1 200 OK\r\nContent-Length: 42\r\nConnection: close\r\n\r\n",
    )
    .await;
    let client = test_client();

    let resp = client.get(&url).send().await.unwrap();
    assert_eq!(resp.content_length(), Some(42));

    handle.await.unwrap();
}

#[tokio::test]
async fn per_request_timeout_override() {
    // Server that never responds
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let url = format!("http://127.0.0.1:{}", addr.port());

    let _handle = tokio::spawn(async move {
        let (mut _stream, _) = listener.accept().await.unwrap();
        tokio::time::sleep(Duration::from_secs(60)).await;
    });

    // Client has long timeout, but request overrides to short
    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build();

    let result = client
        .get(&url)
        .timeout(Duration::from_millis(100))
        .send()
        .await;
    assert!(result.is_err());
}
```

- [ ] **Step 2: Run integration tests**

Run: `cargo test --features http-client --test http_client`
Expected: All integration tests pass.

- [ ] **Step 3: Run full test suite to check no regressions**

Run: `cargo test --features full`
Expected: All tests pass (existing modules still compile and work with transitive `http-client` dependency).

Run: `cargo clippy --features full --tests -- -D warnings`
Expected: No warnings.

- [ ] **Step 4: Commit**

```bash
git add tests/http_client.rs
git commit -m "test(http): add integration tests for HTTP client"
```

---

### Task 6: Consolidate webhook module

**Files:**
- Modify: `src/webhook/client.rs`
- Modify: `src/webhook/sender.rs`
- Modify: `src/webhook/mod.rs`
- Modify: `src/lib.rs` (re-exports)

- [ ] **Step 1: Rewrite `src/webhook/client.rs`**

Remove `HyperClient`, `HttpClient` trait, and all direct hyper imports. Replace with a thin wrapper over `crate::http::Client`:

```rust
use bytes::Bytes;
use http::{HeaderMap, StatusCode};

use crate::error::Result;

/// Response returned after a webhook delivery attempt.
pub struct WebhookResponse {
    /// HTTP status code returned by the endpoint.
    pub status: StatusCode,
    /// Response body bytes.
    pub body: Bytes,
}

/// Send a webhook POST via the shared HTTP client.
pub(crate) async fn post(
    client: &crate::http::Client,
    url: &str,
    headers: HeaderMap,
    body: Bytes,
) -> Result<WebhookResponse> {
    let mut builder = client.post(url);
    builder = builder.headers(headers);
    builder = builder.body(body);

    let response = builder.send().await?;
    let status = response.status();
    let response_body = response.bytes().await?;

    Ok(WebhookResponse {
        status,
        body: response_body,
    })
}
```

- [ ] **Step 2: Rewrite `src/webhook/sender.rs`**

Replace generic `C: HttpClient` with concrete `crate::http::Client`:

```rust
use std::sync::Arc;

use bytes::Bytes;
use http::HeaderMap;

use super::client::{self, WebhookResponse};
use super::secret::WebhookSecret;
use super::signature::sign_headers;
use crate::error::{Error, Result};

struct WebhookSenderInner {
    client: crate::http::Client,
    user_agent: String,
}

/// High-level webhook sender that signs and delivers payloads using the
/// Standard Webhooks protocol.
///
/// Clone-cheap: the inner state is wrapped in `Arc`.
pub struct WebhookSender {
    inner: Arc<WebhookSenderInner>,
}

impl Clone for WebhookSender {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl WebhookSender {
    /// Create a new sender with the given HTTP client.
    pub fn new(client: crate::http::Client) -> Self {
        Self {
            inner: Arc::new(WebhookSenderInner {
                client,
                user_agent: format!("modo-webhooks/{}", env!("CARGO_PKG_VERSION")),
            }),
        }
    }

    /// Override the default `User-Agent` header sent with every request.
    ///
    /// # Panics
    ///
    /// Panics if called after the sender has been cloned. Call this immediately
    /// after [`WebhookSender::new`] before handing clones to other tasks.
    pub fn with_user_agent(mut self, user_agent: impl Into<String>) -> Self {
        let inner =
            Arc::get_mut(&mut self.inner).expect("with_user_agent must be called before cloning");
        inner.user_agent = user_agent.into();
        self
    }

    /// Convenience constructor using a default [`Client`](crate::http::Client).
    pub fn default_client() -> Self {
        use crate::http::Client;
        Self::new(Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build())
    }

    /// Send a webhook following the Standard Webhooks protocol.
    ///
    /// Signs the payload with every secret in `secrets` (supports key rotation)
    /// and POSTs to `url` with the three Standard Webhooks headers:
    /// `webhook-id`, `webhook-timestamp`, and `webhook-signature`.
    pub async fn send(
        &self,
        url: &str,
        id: &str,
        body: &[u8],
        secrets: &[&WebhookSecret],
    ) -> Result<WebhookResponse> {
        if secrets.is_empty() {
            return Err(Error::bad_request("at least one secret required"));
        }
        if id.is_empty() {
            return Err(Error::bad_request("webhook id must not be empty"));
        }
        let _: http::Uri = url
            .parse()
            .map_err(|e| Error::bad_request(format!("invalid webhook url: {e}")))?;

        let timestamp = chrono::Utc::now().timestamp();
        let signed = sign_headers(secrets, id, timestamp, body);

        let mut headers = HeaderMap::new();
        headers.insert("content-type", "application/json".parse().unwrap());
        headers.insert("user-agent", self.inner.user_agent.parse().unwrap());
        headers.insert(
            "webhook-id",
            signed
                .webhook_id
                .parse()
                .map_err(|_| Error::bad_request("webhook id contains invalid header characters"))?,
        );
        headers.insert(
            "webhook-timestamp",
            signed.webhook_timestamp.to_string().parse().unwrap(),
        );
        headers.insert(
            "webhook-signature",
            signed
                .webhook_signature
                .parse()
                .map_err(|_| Error::internal("generated invalid webhook-signature header"))?,
        );

        client::post(&self.inner.client, url, headers, Bytes::copy_from_slice(body)).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::StatusCode;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    async fn start_webhook_server(status: u16) -> (String, tokio::task::JoinHandle<Vec<u8>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let url = format!("http://127.0.0.1:{}", addr.port());

        let handle = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 8192];
            let n = stream.read(&mut buf).await.unwrap();
            let response = format!(
                "HTTP/1.1 {status} OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            );
            stream.write_all(response.as_bytes()).await.unwrap();
            stream.shutdown().await.unwrap();
            buf[..n].to_vec()
        });

        (url, handle)
    }

    fn test_sender() -> WebhookSender {
        let _ = rustls::crypto::ring::default_provider().install_default();
        WebhookSender::default_client()
    }

    #[tokio::test]
    async fn send_sets_correct_headers() {
        let (url, handle) = start_webhook_server(200).await;
        let sender = test_sender();
        let secret = WebhookSecret::new(b"test-key".to_vec());

        let result = sender.send(&url, "msg_123", b"{}", &[&secret]).await;
        assert!(result.is_ok());

        let request_bytes = handle.await.unwrap();
        let request_text = String::from_utf8_lossy(&request_bytes);
        assert!(request_text.contains("webhook-id: msg_123"));
        assert!(request_text.contains("webhook-timestamp:"));
        assert!(request_text.contains("webhook-signature: v1,"));
        assert!(request_text.contains("content-type: application/json"));
    }

    #[tokio::test]
    async fn send_empty_secrets_rejected() {
        let sender = test_sender();
        let result = sender
            .send("http://example.com/hook", "msg_1", b"{}", &[])
            .await;
        assert!(result.is_err());
        assert!(result.err().unwrap().message().contains("secret"));
    }

    #[tokio::test]
    async fn send_empty_id_rejected() {
        let sender = test_sender();
        let secret = WebhookSecret::new(b"key".to_vec());
        let result = sender
            .send("http://example.com/hook", "", b"{}", &[&secret])
            .await;
        assert!(result.is_err());
        assert!(result.err().unwrap().message().contains("id"));
    }

    #[tokio::test]
    async fn send_invalid_url_rejected() {
        let sender = test_sender();
        let secret = WebhookSecret::new(b"key".to_vec());
        let result = sender
            .send("not a valid url", "msg_1", b"{}", &[&secret])
            .await;
        assert!(result.is_err());
        assert!(
            result
                .err()
                .unwrap()
                .message()
                .contains("invalid webhook url")
        );
    }

    #[tokio::test]
    async fn send_returns_response_status() {
        let (url, handle) = start_webhook_server(410).await;
        let sender = test_sender();
        let secret = WebhookSecret::new(b"key".to_vec());

        let response = sender.send(&url, "msg_1", b"{}", &[&secret]).await.unwrap();
        assert_eq!(response.status, StatusCode::GONE);
        handle.await.unwrap();
    }
}
```

- [ ] **Step 3: Update `src/webhook/mod.rs`**

```rust
//! Outbound webhook delivery following the Standard Webhooks specification.
//!
//! This module provides signed outbound HTTP POST requests using HMAC-SHA256.
//! All types require the `"webhooks"` feature.

mod client;
mod secret;
mod sender;
mod signature;

pub use client::WebhookResponse;
pub use secret::WebhookSecret;
pub use sender::WebhookSender;
pub use signature::{SignedHeaders, sign, sign_headers, verify, verify_headers};
```

- [ ] **Step 4: Update re-exports in `src/lib.rs`**

Replace the existing webhooks re-export block:

```rust
#[cfg(feature = "webhooks")]
pub use webhook::{SignedHeaders, WebhookResponse, WebhookSecret, WebhookSender};
```

(Removed `HttpClient` and `HyperClient` — they no longer exist.)

- [ ] **Step 5: Run tests**

Run: `cargo test --features webhooks`
Expected: All webhook tests pass with the new implementation.

Run: `cargo clippy --features webhooks --tests -- -D warnings`
Expected: No warnings.

- [ ] **Step 6: Commit**

```bash
git add src/webhook/ src/lib.rs
git commit -m "refactor(webhook): use http::Client, remove HyperClient and HttpClient trait"
```

---

### Task 7: Consolidate storage module

**Files:**
- Modify: `src/storage/client.rs`
- Modify: `src/storage/backend.rs`
- Modify: `src/storage/fetch.rs`
- Modify: `src/storage/facade.rs`

- [ ] **Step 1: Update `src/storage/client.rs`**

Replace the hyper client construction and raw hyper types with `crate::http::Client`. The S3 signing logic stays — it still builds raw `hyper::Request` objects for signing, but the underlying connection comes from the shared client.

Key changes:
- `RemoteBackend.client` field type changes from verbose hyper generic to `crate::http::Client`
- Remove `HttpsConnectorBuilder`, `hyper_util::client::legacy::Client`, `TokioExecutor` imports
- Keep `hyper::Request`, `hyper::Method` imports — needed for building signed requests
- The `client()` accessor return type changes to `&crate::http::Client`
- `RemoteBackend::new()` takes `client: crate::http::Client` parameter instead of building one internally

The S3 methods (`put`, `delete`, `exists`, `list`) need to send raw pre-signed `hyper::Request` objects through the underlying hyper client. Add a `pub(crate) fn raw_client(&self)` method to `crate::http::Client` that exposes the inner hyper client for this purpose. Add this to `src/http/client.rs`:

```rust
    /// Access the underlying hyper client for advanced use cases.
    ///
    /// This is intended for internal framework modules (like S3 storage) that
    /// need to send pre-built `hyper::Request` objects with custom signing.
    pub(crate) fn raw_client(
        &self,
    ) -> &hyper_util::client::legacy::Client<
        hyper_rustls::HttpsConnector<hyper_util::client::legacy::connect::HttpConnector>,
        http_body_util::Full<bytes::Bytes>,
    > {
        &self.inner.client
    }
```

Then update `RemoteBackend`:

```rust
use std::time::Duration;

use bytes::Bytes;
use http::Uri;
use http_body_util::{BodyExt, Full};

use super::options::PutOptions;
use super::presign::{PresignParams, presign_url};
use super::signing::{SigningParams, sign_request, uri_encode};
use crate::error::{Error, Result};

pub(crate) struct RemoteBackend {
    client: crate::http::Client,
    bucket: String,
    endpoint: String,
    endpoint_host: String,
    access_key: String,
    secret_key: String,
    region: String,
    path_style: bool,
}
```

Update `RemoteBackend::new()` to accept a `crate::http::Client`:

```rust
    pub fn new(
        client: crate::http::Client,
        bucket: String,
        endpoint: String,
        access_key: String,
        secret_key: String,
        region: String,
        path_style: bool,
    ) -> Result<Self> {
        let endpoint_host = strip_scheme(&endpoint).to_string();
        Ok(Self {
            client,
            bucket,
            endpoint,
            endpoint_host,
            access_key,
            secret_key,
            region,
            path_style,
        })
    }
```

In all S3 methods (`put`, `delete`, `exists`, `list`), replace `self.client.request(request)` with `self.client.raw_client().request(request)`.

Update the `client()` accessor:

```rust
    pub(crate) fn client(&self) -> &crate::http::Client {
        &self.client
    }
```

- [ ] **Step 2: Update `src/storage/backend.rs`**

Replace the verbose hyper type with `crate::http::Client`:

```rust
use super::client::RemoteBackend;
use super::memory::MemoryBackend;
use crate::error::{Error, Result};

pub(crate) enum BackendKind {
    Remote(Box<RemoteBackend>),
    #[cfg_attr(not(any(test, feature = "storage-test")), allow(dead_code))]
    Memory(MemoryBackend),
}

impl BackendKind {
    pub(crate) fn http_client(&self) -> Result<&crate::http::Client> {
        match self {
            BackendKind::Remote(b) => Ok(b.client()),
            BackendKind::Memory(_) => {
                Err(Error::internal("URL fetch not supported in memory backend"))
            }
        }
    }
}
```

- [ ] **Step 3: Update `src/storage/fetch.rs`**

Replace the verbose hyper client type with `crate::http::Client`:

```rust
use std::time::Duration;

use bytes::Bytes;
use http::Uri;
use http_body_util::{BodyExt, Full};

use crate::error::{Error, Result};

pub(crate) struct FetchResult {
    pub data: Bytes,
    pub content_type: String,
}

fn validate_url(url: &str) -> Result<Uri> {
    let uri: Uri = url
        .parse()
        .map_err(|e| Error::bad_request(format!("invalid URL: {e}")))?;
    match uri.scheme_str() {
        Some("http") | Some("https") => Ok(uri),
        Some(scheme) => Err(Error::bad_request(format!(
            "URL must use http or https scheme, got {scheme}"
        ))),
        None => Err(Error::bad_request("URL must use http or https scheme")),
    }
}

pub(crate) async fn fetch_url(
    client: &crate::http::Client,
    url: &str,
    max_size: Option<usize>,
) -> Result<FetchResult> {
    let uri = validate_url(url)?;

    let request = hyper::Request::builder()
        .method(hyper::Method::GET)
        .uri(&uri)
        .body(Full::new(Bytes::new()))
        .map_err(|e| Error::internal(format!("failed to build request: {e}")))?;

    let response = tokio::time::timeout(
        Duration::from_secs(30),
        client.raw_client().request(request),
    )
    .await
    .map_err(|_| Error::internal("URL fetch timed out"))?
    .map_err(|e| Error::internal(format!("failed to fetch URL: {e}")))?;

    let status = response.status();
    if !status.is_success() {
        return Err(Error::bad_request(format!(
            "failed to fetch URL ({status})"
        )));
    }

    let content_type = response
        .headers()
        .get(http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/octet-stream")
        .to_string();

    let mut body = response.into_body();
    let mut buf: Vec<u8> = Vec::new();

    loop {
        let frame = match std::pin::pin!(body.frame()).await {
            Some(Ok(frame)) => frame,
            Some(Err(e)) => {
                return Err(Error::internal(format!(
                    "failed to read response body: {e}"
                )));
            }
            None => break,
        };

        if let Some(chunk) = frame.data_ref() {
            buf.extend_from_slice(chunk);
            if let Some(max) = max_size
                && buf.len() > max
            {
                return Err(Error::payload_too_large(format!(
                    "fetched file size exceeds maximum {max}"
                )));
            }
        }
    }

    Ok(FetchResult {
        data: Bytes::from(buf),
        content_type,
    })
}
```

Tests in `fetch.rs` need updating — the `build_test_client()` helper changes to return `crate::http::Client`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    fn build_test_client() -> crate::http::Client {
        let _ = rustls::crypto::ring::default_provider().install_default();
        crate::http::Client::default()
    }

    // All test functions stay the same but use `build_test_client()` which now
    // returns `crate::http::Client`. The `fetch_url` signature now takes
    // `&crate::http::Client` so the call sites are unchanged.
    // ... (keep all existing test functions, they work as-is with the new type)
}
```

- [ ] **Step 4: Update `src/storage/facade.rs`**

Update `Storage::new()` to create a `crate::http::Client` and pass it to `RemoteBackend::new()`:

```rust
    pub fn new(config: &BucketConfig) -> Result<Self> {
        config.validate()?;

        let http_client = crate::http::Client::default();
        let region = config
            .region
            .clone()
            .unwrap_or_else(|| "us-east-1".to_string());
        let backend = RemoteBackend::new(
            http_client,
            config.bucket.clone(),
            config.endpoint.clone(),
            config.access_key.clone(),
            config.secret_key.clone(),
            region,
            config.path_style,
        )?;

        Ok(Self {
            inner: Arc::new(StorageInner {
                backend: BackendKind::Remote(Box::new(backend)),
                public_url: config.normalized_public_url(),
                max_file_size: config.max_file_size_bytes()?,
            }),
        })
    }
```

- [ ] **Step 5: Run tests**

Run: `cargo test --features storage`
Expected: All storage tests pass.

Run: `cargo test --features storage-test`
Expected: All storage integration tests pass.

Run: `cargo clippy --features storage --tests -- -D warnings`
Expected: No warnings.

- [ ] **Step 6: Commit**

```bash
git add src/http/client.rs src/storage/
git commit -m "refactor(storage): use http::Client, remove direct hyper client setup"
```

---

### Task 8: Consolidate auth::oauth module

**Files:**
- Modify: `src/auth/oauth/client.rs`
- Modify: `src/auth/oauth/google.rs`
- Modify: `src/auth/oauth/github.rs`

- [ ] **Step 1: Rewrite `src/auth/oauth/client.rs`**

Replace the fresh-client-per-call pattern with `crate::http::Client`:

```rust
//! Internal HTTP helpers used by provider implementations.
//!
//! Functions take a shared `http::Client` reference, gaining connection pooling
//! via the framework-wide HTTP client.

use serde::de::DeserializeOwned;

pub(crate) async fn post_form<T: DeserializeOwned>(
    client: &crate::http::Client,
    url: &str,
    params: &[(&str, &str)],
) -> crate::Result<T> {
    client
        .post(url)
        .form(&params)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await
}

pub(crate) async fn get_json<T: DeserializeOwned>(
    client: &crate::http::Client,
    url: &str,
    token: &str,
) -> crate::Result<T> {
    client
        .get(url)
        .bearer_token(token)
        .header(
            http::header::ACCEPT,
            http::header::HeaderValue::from_static("application/json"),
        )
        .send()
        .await?
        .error_for_status()?
        .json()
        .await
}
```

- [ ] **Step 2: Update `src/auth/oauth/google.rs`**

Add `http_client: crate::http::Client` field to `Google` struct and pass it through to `client::post_form` and `client::get_json`:

In the struct:
```rust
pub struct Google {
    config: OAuthProviderConfig,
    cookie_config: CookieConfig,
    key: Key,
    http_client: crate::http::Client,
}
```

In `Google::new()`:
```rust
    pub fn new(
        config: &OAuthProviderConfig,
        cookie_config: &CookieConfig,
        key: &Key,
        http_client: crate::http::Client,
    ) -> Self {
        Self {
            config: config.clone(),
            cookie_config: cookie_config.clone(),
            key: key.clone(),
            http_client,
        }
    }
```

In `exchange()`, update the calls:
```rust
        let token: TokenResponse = client::post_form(
            &self.http_client,
            TOKEN_URL,
            &[
                ("grant_type", "authorization_code"),
                ("code", &params.code),
                ("redirect_uri", &self.config.redirect_uri),
                ("client_id", &self.config.client_id),
                ("client_secret", &self.config.client_secret),
                ("code_verifier", state.pkce_verifier()),
            ],
        )
        .await?;

        let raw: serde_json::Value =
            client::get_json(&self.http_client, USERINFO_URL, &token.access_token).await?;
```

- [ ] **Step 3: Update `src/auth/oauth/github.rs`**

Same pattern as Google — add `http_client: crate::http::Client` field:

In the struct:
```rust
pub struct GitHub {
    config: OAuthProviderConfig,
    cookie_config: CookieConfig,
    key: Key,
    http_client: crate::http::Client,
}
```

In `GitHub::new()`:
```rust
    pub fn new(
        config: &OAuthProviderConfig,
        cookie_config: &CookieConfig,
        key: &Key,
        http_client: crate::http::Client,
    ) -> Self {
        Self {
            config: config.clone(),
            cookie_config: cookie_config.clone(),
            key: key.clone(),
            http_client,
        }
    }
```

In `exchange()`, update the three calls:
```rust
        let token: TokenResponse = client::post_form(
            &self.http_client,
            TOKEN_URL,
            &[...],
        )
        .await?;

        let raw: serde_json::Value =
            client::get_json(&self.http_client, USER_URL, &token.access_token).await?;

        // ...

        let emails: Vec<GitHubEmail> =
            client::get_json(&self.http_client, EMAILS_URL, &token.access_token).await?;
```

- [ ] **Step 4: Run tests**

Run: `cargo test --features auth`
Expected: All auth tests pass.

Run: `cargo clippy --features auth --tests -- -D warnings`
Expected: No warnings.

- [ ] **Step 5: Commit**

```bash
git add src/auth/oauth/
git commit -m "refactor(oauth): use http::Client, remove per-call hyper client creation"
```

---

### Task 9: Final Verification

**Files:**
- No new files

- [ ] **Step 1: Run full test suite**

Run: `cargo test --features full`
Expected: All tests pass.

- [ ] **Step 2: Run full lint**

Run: `cargo clippy --features full --tests -- -D warnings`
Expected: No warnings.

- [ ] **Step 3: Run format check**

Run: `cargo fmt --check`
Expected: No formatting issues.

- [ ] **Step 4: Verify each feature compiles independently**

Run these in parallel:
```
cargo check --features http-client
cargo check --features auth
cargo check --features storage
cargo check --features webhooks
cargo check --features "auth,storage"
cargo check --features "auth,webhooks"
cargo check --features full
```
Expected: All compile without errors.

- [ ] **Step 5: Verify no leftover hyper imports in consolidated modules**

Grep for direct hyper imports in the three consolidated modules:
```
rg "use hyper_rustls" src/webhook/ src/auth/oauth/ src/storage/backend.rs
rg "use hyper_util::client" src/webhook/ src/auth/oauth/ src/storage/backend.rs
rg "HttpsConnectorBuilder" src/webhook/ src/auth/oauth/ src/storage/backend.rs
```
Expected: No matches (only `src/http/` and `src/storage/client.rs` + `src/storage/fetch.rs` should have hyper imports for S3 signing).

- [ ] **Step 6: Commit (if any fmt fixes were needed)**

```bash
cargo fmt
git add -A
git commit -m "chore: format code"
```
