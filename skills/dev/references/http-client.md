# HTTP Client

Feature flag: `http-client`

```toml
[dependencies]
modo = { version = "*", features = ["http-client"] }
```

HTTP client with connection pooling, TLS (rustls), timeouts, and automatic retries with exponential backoff. Built on hyper + hyper-rustls. Dependencies: `dep:hyper`, `dep:hyper-rustls`, `dep:hyper-util`, `dep:http-body-util`, `dep:base64`.

## Public API

Re-exported at crate root when the `http-client` feature is enabled:

```rust
pub use http::{
    Client as HttpClient, ClientBuilder as HttpClientBuilder, ClientConfig as HttpClientConfig,
};
```

Module-level re-exports (from `src/http/mod.rs`):

```rust
pub use client::{Client, ClientBuilder};
pub use config::ClientConfig;
pub use request::RequestBuilder;
pub use response::{BodyStream, Response};
```

---

## ClientConfig

`#[non_exhaustive]` -- use `..Default::default()` for forward compatibility. Deserializes from YAML via serde (`serde_yaml_ng`, not `serde_yaml`).

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct ClientConfig {
    pub timeout_ms: u64,          // default: 30_000 (30s). 0 = no timeout.
    pub connect_timeout_ms: u64,  // default: 5_000 (5s). 0 = no connect timeout.
    pub user_agent: String,       // default: "modo/0.1"
    pub max_retries: u32,         // default: 0 (no retries)
    pub retry_backoff_ms: u64,    // default: 100. Actual delay = retry_backoff_ms * 2^attempt.
}
```

Implements `Default` with the values shown above.

YAML example:

```yaml
http:
    timeout_ms: 10000
    connect_timeout_ms: 2000
    user_agent: "myapp/1.0"
    max_retries: 3
    retry_backoff_ms: 200
```

All fields are optional in YAML -- omitted fields use defaults.

---

## Client

A reusable HTTP client. Wraps state in `Arc` -- cheap to clone. All clones share the same connection pool and configuration.

```rust
#[derive(Clone)]
pub struct Client { /* Arc<ClientInner> */ }
```

Implements `Default` (equivalent to `Client::new(&ClientConfig::default())`).

### new(config)

```rust
pub fn new(config: &ClientConfig) -> Self
```

Create a new client from the given configuration. Sets up an HTTPS connector with webpki roots, HTTP/1.1, and the configured connect timeout.

### builder()

```rust
pub fn builder() -> ClientBuilder
```

Start building a client with a `ClientBuilder` for fluent configuration.

### get(url), post(url), put(url), patch(url), delete(url)

```rust
pub fn get(&self, url: &str) -> RequestBuilder
pub fn post(&self, url: &str) -> RequestBuilder
pub fn put(&self, url: &str) -> RequestBuilder
pub fn patch(&self, url: &str) -> RequestBuilder
pub fn delete(&self, url: &str) -> RequestBuilder
```

Start a request with the corresponding HTTP method. Returns a `RequestBuilder` for further configuration before sending.

### request(method, url)

```rust
pub fn request(&self, method: http::Method, url: &str) -> RequestBuilder
```

Start a request with an arbitrary HTTP method and URL. The convenience methods (`get`, `post`, etc.) delegate to this.

---

## ClientBuilder

Fluent builder for constructing a `Client`. Starts from `ClientConfig::default()` values.

```rust
pub struct ClientBuilder { /* config: ClientConfig */ }
```

### timeout(d)

```rust
pub fn timeout(self, d: Duration) -> Self
```

Set the default request timeout.

### connect_timeout(d)

```rust
pub fn connect_timeout(self, d: Duration) -> Self
```

Set the TCP connect timeout.

### user_agent(ua)

```rust
pub fn user_agent(self, ua: impl Into<String>) -> Self
```

Set the `User-Agent` header value.

### max_retries(n)

```rust
pub fn max_retries(self, n: u32) -> Self
```

Set the maximum number of retry attempts for retryable failures.

### retry_backoff(d)

```rust
pub fn retry_backoff(self, d: Duration) -> Self
```

Set the initial retry backoff duration.

### build()

```rust
pub fn build(self) -> Client
```

Build the client. Delegates to `Client::new`.

Usage example:

```rust
use std::time::Duration;
use modo::HttpClient;

let client = HttpClient::builder()
    .timeout(Duration::from_secs(10))
    .connect_timeout(Duration::from_secs(2))
    .user_agent("myapp/1.0")
    .max_retries(3)
    .retry_backoff(Duration::from_millis(200))
    .build();
```

---

## RequestBuilder

Per-request builder with headers, auth, body, query params, and retry overrides. Obtained from `Client::get`, `Client::post`, etc. Errors from URL parsing, header construction, and body serialization are deferred until `send()` is called.

```rust
pub struct RequestBuilder { /* inner fields */ }
```

### header(name, value)

```rust
pub fn header(self, name: http::header::HeaderName, value: http::header::HeaderValue) -> Self
```

Set a single header, replacing any existing value with the same name.

### headers(headers)

```rust
pub fn headers(self, headers: HeaderMap) -> Self
```

Merge additional headers into the request.

### bearer_token(token)

```rust
pub fn bearer_token(self, token: impl AsRef<str>) -> Self
```

Set a `Bearer` authorization header. If the token produces an invalid header value, the error is deferred until `send()`.

### basic_auth(username, password)

```rust
pub fn basic_auth(self, username: &str, password: Option<&str>) -> Self
```

Set a `Basic` authorization header. Encodes `username:password` with standard base64. When `password` is `None`, encodes `username:`. Invalid header values are deferred until `send()`.

### query(params)

```rust
pub fn query(self, params: &[(&str, &str)]) -> Self
```

Append query parameters to the URL. Parameters are percent-encoded (RFC 3986 unreserved characters preserved) and appended to any existing query string. Uses `&` as separator when the URL already contains a `?`.

### json(value)

```rust
pub fn json(self, value: &impl Serialize) -> Self
```

Set a JSON request body. Also sets `Content-Type: application/json`. Serialization errors are deferred until `send()`.

### form(body)

```rust
pub fn form<T: Serialize>(self, body: &T) -> Self
```

Set a URL-encoded form body. Also sets `Content-Type: application/x-www-form-urlencoded`. Serialization errors are deferred until `send()`.

### body(bytes)

```rust
pub fn body(self, bytes: impl Into<Bytes>) -> Self
```

Set a raw byte body.

### timeout(duration)

```rust
pub fn timeout(self, duration: Duration) -> Self
```

Override the request timeout for this request only.

### max_retries(n)

```rust
pub fn max_retries(self, n: u32) -> Self
```

Override the maximum retry count for this request only.

### async send()

```rust
pub async fn send(self) -> Result<Response>
```

Send the request and return the response. Evaluates all deferred errors (URL parsing, auth headers, body serialization) before dispatching. Applies the default `User-Agent` header if not explicitly set. Executes the retry policy if configured.

Usage example:

```rust
let resp = client
    .post("https://api.example.com/items")
    .bearer_token("tok_abc")
    .json(&payload)
    .query(&[("dry_run", "true")])
    .timeout(Duration::from_secs(5))
    .max_retries(2)
    .send()
    .await?;
```

---

## Response

An HTTP response received from an outgoing request. Provides typed accessors for status, headers, and URL, plus terminal methods to consume the body.

```rust
pub struct Response { /* status, headers, url, body */ }
```

### status()

```rust
pub fn status(&self) -> StatusCode
```

Returns the HTTP status code.

### headers()

```rust
pub fn headers(&self) -> &HeaderMap
```

Returns the response headers.

### url()

```rust
pub fn url(&self) -> &str
```

Returns the URL that produced this response.

### content_length()

```rust
pub fn content_length(&self) -> Option<u64>
```

Returns the `Content-Length` header value, if present and valid.

### async json()

```rust
pub async fn json<T: DeserializeOwned>(self) -> Result<T>
```

Consume the body and deserialize it as JSON. Returns `Error::internal` (500) on parse failure.

### async text()

```rust
pub async fn text(self) -> Result<String>
```

Consume the body and return it as a UTF-8 string. Returns `Error::internal` (500) if the body is not valid UTF-8.

### async bytes()

```rust
pub async fn bytes(self) -> Result<Bytes>
```

Consume the body and return it as raw bytes. Returns `Error::internal` (500) on read failure.

### stream()

```rust
pub fn stream(self) -> BodyStream
```

Convert this response into a streaming body reader. Consumes the response.

### error_for_status()

```rust
pub fn error_for_status(self) -> Result<Self>
```

Return `Ok(self)` if the status is 2xx, otherwise return `Error::internal` with message `"HTTP {status}: {url}"`. Call this before consuming the body to avoid reading error bodies unnecessarily.

Usage example:

```rust
let resp = client.get("https://api.example.com/data").send().await?;
let data: MyStruct = resp.error_for_status()?.json().await?;
```

---

## BodyStream

A streaming reader over a response body. Each call to `next()` yields the next data frame as raw bytes, skipping trailers and other non-data frames.

```rust
pub struct BodyStream { /* body: hyper::body::Incoming */ }
```

### async next()

```rust
pub async fn next(&mut self) -> Option<Result<Bytes>>
```

Read the next data frame from the body stream. Returns `None` when the stream is complete. Non-data frames (trailers) are silently skipped. Returns `Some(Err(...))` on read failure.

Usage example:

```rust
let mut stream = resp.stream();
while let Some(chunk) = stream.next().await {
    let bytes = chunk?;
    // process bytes...
}
```

---

## Gotchas

### Retry behaviour

- Retryable statuses: `429 Too Many Requests`, `502 Bad Gateway`, `503 Service Unavailable`.
- Retryable errors: timeouts and connection failures.
- Non-retryable: all other HTTP statuses (including 4xx) and fatal hyper errors.
- `Retry-After` header is respected for 429 responses (integer seconds or RFC 2822 date), capped at 60 seconds.
- Exponential backoff formula: `retry_backoff_ms * 2^attempt` (starting at attempt 0).
- When retries are exhausted and the last attempt was a retryable HTTP response (not an error), the response is returned rather than an error. This lets callers inspect the 429/502/503 body if needed.
- `max_retries: 0` (the default) means no retries -- the request is sent exactly once.

### Deferred errors

`RequestBuilder` methods like `bearer_token`, `basic_auth`, `json`, `form`, and `query` never return `Result`. Instead, errors are stored internally and surfaced when `send()` is called. This keeps the builder chain ergonomic.

### User-Agent default

The `User-Agent` header from `ClientConfig` is applied only if the caller has not explicitly set one via `header()` or `headers()`.

### TLS and HTTP version

The client uses rustls with webpki roots (no system certificates). Only HTTP/1.1 is enabled. Both `https://` and `http://` URLs are supported.

### Body re-creation on retry

The request body is cloned for each retry attempt because hyper consumes the body on send. The body bytes (`Bytes`) are cheaply cloneable (reference-counted).

### Crate re-export names

At the crate root, types are re-exported with `Http` prefix to avoid name collisions: `HttpClient`, `HttpClientBuilder`, `HttpClientConfig`. Within `modo::http::*` they use their original names (`Client`, `ClientBuilder`, `ClientConfig`).

### Module files

`client.rs`, `config.rs`, `request.rs`, `response.rs`, `retry.rs`. `mod.rs` is only re-exports.
