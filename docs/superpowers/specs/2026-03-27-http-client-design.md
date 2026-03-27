# HTTP Client Module Design — 2026-03-27

Ergonomic async HTTP client built on hyper/hyper-rustls. Single shared client replacing per-module HTTP implementations in `storage`, `webhook`, and `auth::oauth`.

## Approach

Thin wrapper over `hyper_util::client::legacy::Client`. RequestBuilder + Response types, retry loop with tracing, timeout handling. No new dependencies — hyper stack is already in the dep tree. Consolidates three independent connector/pool setups into one.

## Feature Flag

`http-client` — pulls in `hyper`, `hyper-rustls`, `hyper-util`, `http-body-util`.

Modules that need HTTP depend on `http-client` in their feature definitions:

```toml
# Cargo.toml
http-client = ["dep:hyper", "dep:hyper-rustls", "dep:hyper-util", "dep:http-body-util"]
auth = ["http-client", ...]      # replaces direct hyper deps
storage = ["http-client", ...]
webhooks = ["http-client", ...]
```

Apps that don't use any HTTP-dependent module pay no cost.

Top-level `Config` struct gets:

```rust
#[cfg(feature = "http-client")]
#[serde(default)]
pub http: crate::http::ClientConfig,
```

## Module Structure

```
src/http/
  mod.rs          — mod imports + re-exports
  client.rs       — Client struct (Arc<Inner>), constructors, method dispatchers
  request.rs      — RequestBuilder (headers, body, auth, query, timeout, send)
  response.rs     — Response (status, headers, json, text, bytes, stream, error_for_status)
  config.rs       — ClientConfig (YAML deserialization)
  retry.rs        — RetryPolicy + retry loop with tracing spans
```

No `error.rs` file — all errors use `modo::Error` constructors directly (`.internal()`, `.bad_request()`) with `.chain()` for source errors. Error creation is inline in each file where it occurs.

## Client

### Inner Structure

```rust
struct ClientInner {
    client: hyper_util::client::legacy::Client<
        hyper_rustls::HttpsConnector<hyper_util::client::legacy::connect::HttpConnector>,
        http_body_util::Full<bytes::Bytes>,
    >,
    config: ClientConfig,
}

pub struct Client {
    inner: Arc<ClientInner>,
}

impl Clone for Client { /* Arc::clone */ }
```

Cheaply cloneable. Single connection pool shared across all clones and all framework modules.

### Constructors

```rust
impl Client {
    /// Primary — from config struct.
    pub fn new(config: &ClientConfig) -> Client

    /// Default config.
    pub fn default() -> Client

    /// Fine-grained programmatic construction.
    pub fn builder() -> ClientBuilder
}
```

`Client::new(&ClientConfig)` is the single place that builds the hyper connector and `Arc<ClientInner>`. Both `default()` and `builder().build()` funnel through it.

### Connector Setup

```rust
let mut http_connector = HttpConnector::new();
http_connector.set_connect_timeout(Some(connect_timeout));

let connector = HttpsConnectorBuilder::new()
    .with_webpki_roots()
    .https_or_http()
    .enable_http1()
    .wrap_connector(http_connector);

let client = hyper_util::client::legacy::Client::builder(TokioExecutor::new())
    .build(connector);
```

Matches existing modo connector setup. HTTP/1.1 only, webpki roots, both http and https schemes.

### Method Dispatchers

Each returns `RequestBuilder`:

```rust
client.get(url)
client.post(url)
client.put(url)
client.patch(url)
client.delete(url)
client.request(method, url)   // escape hatch
```

URL parsing is deferred — invalid URLs become errors at `.send()` time. This keeps builder chaining ergonomic (no `?` after the method call).

## RequestBuilder

```rust
pub struct RequestBuilder {
    client: Arc<ClientInner>,
    method: Method,
    url: Result<Uri>,           // deferred error
    headers: HeaderMap,
    body: Option<Bytes>,
    timeout: Option<Duration>,  // per-request override
    max_retries: Option<u32>,   // per-request override
}
```

### Chaining Methods (all `-> Self`)

```rust
.header(name, value)           // single header
.headers(HeaderMap)            // merge multiple
.bearer_token(token)           // Authorization: Bearer {token}
.basic_auth(user, pass)        // Authorization: Basic {base64}
.query(&[(k, v)])              // append query params to URL
.json(&body)                   // serialize T, set Content-Type: application/json
.form(&body)                   // url-encode T, set Content-Type: application/x-www-form-urlencoded
.body(bytes)                   // raw Bytes body
.timeout(Duration)             // override client-level timeout
.max_retries(n)                // override client-level retry count
```

### Terminal Method

```rust
.send() -> Result<Response>
```

`send()` does:
1. Check deferred URL error
2. Build `hyper::Request` from accumulated state
3. Apply default User-Agent header if not set by caller
4. Enter retry loop (delegates to `retry.rs`)
5. Each attempt: clone request, send via hyper client, wrap in `tokio::time::timeout`
6. Return `Ok(Response)` for any completed HTTP response, or `Err` on connection/timeout failure after retries exhausted

### Deferred Errors

`.json()` and `.form()` serialize eagerly and store as `Bytes`. Serialization errors are deferred and surface at `.send()`. Invalid URLs from the method dispatcher are also deferred. All deferred errors surface at `.send()` — they never silently disappear.

### Notes

- `.query()` appends to existing query string, does not replace
- `.basic_auth()` takes `Option<&str>` for password (some APIs use username-only basic auth)

## Response

```rust
pub struct Response {
    status: StatusCode,
    headers: HeaderMap,
    url: String,
    body: hyper::body::Incoming,
}
```

### Methods

```rust
.status() -> StatusCode
.headers() -> &HeaderMap
.content_length() -> Option<u64>

// Body consumers (each consumes self — one-shot):
.json<T: DeserializeOwned>(self) -> Result<T>
.text(self) -> Result<String>
.bytes(self) -> Result<Bytes>
.stream(self) -> impl Stream<Item = Result<Bytes>>

// Convenience:
.error_for_status(self) -> Result<Response>
```

### Body Consumption

- `.json()`, `.text()`, `.bytes()` collect the full body using `BodyExt::collect().await`
- `.stream()` returns a wrapper that yields frames via `BodyExt::frame()` loop — no full buffering
- All consumers take `self` — body consumed exactly once. Calling `.json()` after `.text()` is a compile-time error

### error_for_status()

- 2xx → returns `Ok(self)`
- non-2xx → returns `Err(Error::internal("HTTP {status}: {url}"))`

Callers who need fine-grained status handling check `.status()` directly and build their own error. `error_for_status()` is a convenience for the common case.

### No Redirect Following

Redirects are not followed. 3xx responses are returned as-is. This matches current modo behavior — webhook delivery and S3 operations treat redirects as errors, not expected flow.

## Retry Policy

```rust
struct RetryPolicy {
    max_retries: u32,
    backoff_ms: u64,
}
```

### Retryable Conditions

- Connection errors (refused, reset, DNS failure)
- Timeouts
- HTTP 502, 503
- HTTP 429 (with Retry-After header respect)

### Not Retried

- 4xx (except 429)
- 5xx other than 502/503
- TLS handshake failures
- Request serialization errors

### Retry Loop

```
for attempt in 0..=max_retries:
    tracing::debug!(attempt, url, method, "http.request")

    result = timeout(duration, client.request(req)).await

    match result:
        connection error or timeout → retryable
        status 502, 503 → retryable
        status 429 →
            read Retry-After header, sleep (capped at 60s)
            if no Retry-After → use normal backoff
            retryable
        any other response → return Ok(Response)
        non-retryable error → return Err immediately

    if attempt < max_retries:
        sleep(backoff_ms * 2^attempt)
        tracing::warn!(attempt, status, "http.retry")
```

### Backoff

Exponential: `backoff_ms * 2^attempt` (100ms → 200ms → 400ms → ...).

### Exhausted Retries

- If last failure was connection/timeout → return `Err`
- If last failure was retryable HTTP status (502, 503, 429) → return `Ok(Response)` with that status. The caller decides what to do

### Observability

Transparent to the caller. Each attempt gets a `tracing::debug!` event. Retries get a `tracing::warn!` event with the reason. No API surface for retry outcomes.

### Request Cloning

Method + URL + headers are cheap to clone. Body is `Bytes` (refcounted via Arc internally), also cheap. `RequestBuilder` always stores body as `Bytes`, so retries are always safe.

## Configuration

```rust
#[non_exhaustive]
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ClientConfig {
    pub timeout_secs: u64,           // default: 30
    pub connect_timeout_secs: u64,   // default: 5
    pub user_agent: String,          // default: "modo/0.1"
    pub max_retries: u32,            // default: 0 (no retries)
    pub retry_backoff_ms: u64,       // default: 100
}
```

### YAML

```yaml
http:
  timeout_secs: 30
  connect_timeout_secs: 5
  user_agent: "myapp/1.0"
  max_retries: 3
  retry_backoff_ms: 200
```

### ClientBuilder

For programmatic construction:

```rust
pub struct ClientBuilder {
    config: ClientConfig,
}

impl ClientBuilder {
    pub fn timeout(mut self, d: Duration) -> Self
    pub fn connect_timeout(mut self, d: Duration) -> Self
    pub fn user_agent(mut self, ua: impl Into<String>) -> Self
    pub fn max_retries(mut self, n: u32) -> Self
    pub fn retry_backoff(mut self, d: Duration) -> Self
    pub fn build(self) -> Client
}
```

`build()` calls `Client::new(&self.config)` internally.

### Zero Timeout

If `timeout_secs == 0`, no `tokio::time::timeout` wrapper is applied — requests have no timeout (rely on TCP/TLS defaults).

## Error Handling

All errors use `modo::Error` with `.chain(source_err)` to preserve the original error.

| Failure | Error | Phase |
|---------|-------|-------|
| Invalid URL | `Error::bad_request("invalid URL: {url}")` | `.send()` (deferred) |
| JSON/form serialization | `Error::internal("failed to serialize request body: {err}")` | `.send()` (deferred) |
| Connection failure | `Error::internal("HTTP connection failed: {err}")` | `.send()` after retries |
| Timeout | `Error::internal("HTTP request timed out")` | `.send()` after retries |
| TLS failure | `Error::internal("HTTP connection failed: {err}")` | `.send()` (not retried) |
| Response body read | `Error::internal("failed to read response body: {err}")` | `.json()`, `.text()`, `.bytes()` |
| JSON deserialization | `Error::internal("failed to parse response JSON: {err}")` | `.json::<T>()` |
| UTF-8 decode | `Error::internal("response body is not valid UTF-8")` | `.text()` |
| `error_for_status()` | `Error::internal("HTTP {status}: {url}")` | explicit caller opt-in |

No `error_code` — HTTP client errors are internal plumbing, not API-response-boundary errors. Callers who need specific error identity check `.status()` on the `Response`.

## Consolidation

After the `http` module ships, three modules are simplified. No backward compatibility needed.

### webhook

Remove `HttpClient` trait and `HyperClient`. `WebhookSender` takes `http::Client` directly:

```rust
struct WebhookSenderInner {
    client: http::Client,
    user_agent: String,
}
```

All direct hyper/hyper-rustls/hyper-util imports removed from `src/webhook/`.

### storage

`RemoteBackend` uses `http::Client` instead of raw hyper client:

```rust
pub(crate) struct RemoteBackend {
    client: http::Client,
    // bucket, endpoint, keys, etc. unchanged
}
```

`fetch_url()` takes `&http::Client` instead of `&Client<HttpsConnector<HttpConnector>, Full<Bytes>>`.

All connector setup and raw hyper types removed from `src/storage/`.

### auth::oauth

`post_form()` and `get_json()` take `&http::Client` as a parameter instead of building a fresh hyper client per call. They gain connection pooling for free:

```rust
pub(crate) async fn post_form<T: DeserializeOwned>(
    client: &http::Client,
    url: &str,
    params: &[(&str, &str)],
) -> Result<T> {
    client.post(url)
        .form(&params)
        .send().await?
        .error_for_status()?
        .json().await
}
```

All direct hyper imports removed from `src/auth/oauth/`.

### Feature Flag Cleanup

`auth`, `storage`, `webhooks` features switch from listing individual hyper deps to depending on `http-client`. The direct `dep:hyper`, `dep:hyper-rustls`, `dep:hyper-util`, `dep:http-body-util` entries move under `http-client` only.

## Testing

### Unit Tests (in `src/http/`)

- `config.rs` — `ClientConfig` deserialization: defaults, custom values, zero-timeout behavior
- `request.rs` — header accumulation, query param appending, deferred URL error, `.json()`/`.form()` serialization
- `retry.rs` — backoff calculation, Retry-After parsing, max retry capping

### Integration Tests (`tests/http_client.rs`, `#![cfg(feature = "http-client")]`)

Use a local `tokio::net::TcpListener` as a test HTTP server (no external deps):

- GET/POST with JSON body round-trip
- Timeout behavior (server that never responds)
- Retry on 503 (server returns 503 twice then 200)
- Retry on 429 with Retry-After header
- Connection refused (nothing listening)
- `.error_for_status()` on 4xx/5xx
- `.stream()` yields chunks correctly
- Large response body via `.bytes()`

### Test Helper (`http-client-test` companion feature)

`http::test::mock_server()` — spins up a local TCP listener returning configurable responses.
