# modo::http

HTTP client with connection pooling, timeouts, and automatic retries.

Requires the `http-client` feature flag.

```toml
[dependencies]
modo = { version = "0.5", features = ["http-client"] }
```

The primary types are also re-exported at the crate root as `HttpClient`,
`HttpClientBuilder`, and `HttpClientConfig`.

## Feature Flag

| Feature flag  | What it enables                                                       |
| ------------- | --------------------------------------------------------------------- |
| `http-client` | `Client`, `ClientBuilder`, `ClientConfig`, `RequestBuilder`, `Response`, `BodyStream` |

## Key Types

| Type              | Description                                                                         |
| ----------------- | ----------------------------------------------------------------------------------- |
| `Client`          | Reusable HTTP client; cheaply cloneable via `Arc`, shared connection pool            |
| `ClientBuilder`   | Fluent builder for constructing a `Client` with custom timeouts, retries, user-agent |
| `ClientConfig`    | Configuration struct; deserializes from the `http` YAML config section               |
| `RequestBuilder`  | Per-request builder with headers, auth, body, query params, and retry overrides      |
| `Response`        | Received HTTP response; consume body as JSON, text, bytes, or a stream               |
| `BodyStream`      | Streaming reader over a response body; yields data frames as raw bytes               |

## Configuration

Add an `http` section to your application YAML config:

```yaml
http:
    timeout_ms: 30000
    connect_timeout_ms: 5000
    user_agent: "myapp/1.0"
    max_retries: 3
    retry_backoff_ms: 100
```

All fields have sensible defaults and the section can be omitted entirely:

| Field              | Default      | Description                                     |
| ------------------ | ------------ | ----------------------------------------------- |
| `timeout_ms`       | `30000`      | Default request timeout in ms (`0` = no timeout) |
| `connect_timeout_ms` | `5000`     | TCP connect timeout in ms (`0` = no timeout)     |
| `user_agent`       | `"modo/0.1"` | Default `User-Agent` header value                |
| `max_retries`      | `0`          | Maximum retry attempts (`0` = no retries)        |
| `retry_backoff_ms` | `100`        | Initial backoff between retries in ms            |

## Usage

### Creating a client

From configuration:

```rust,ignore
use modo::http::{Client, ClientConfig};

let config: ClientConfig = serde_yaml_ng::from_str("timeout_ms: 10000").unwrap();
let client = Client::new(&config);
```

With the builder:

```rust,ignore
use std::time::Duration;
use modo::http::Client;

let client = Client::builder()
    .timeout(Duration::from_secs(10))
    .connect_timeout(Duration::from_secs(2))
    .user_agent("myapp/1.0")
    .max_retries(3)
    .retry_backoff(Duration::from_millis(200))
    .build();
```

### Sending requests

```rust,ignore
// GET with bearer auth
let resp = client
    .get("https://api.example.com/items")
    .bearer_token("tok_abc")
    .send()
    .await?;
let data: Vec<Item> = resp.error_for_status()?.json().await?;

// POST with JSON body
let resp = client
    .post("https://api.example.com/items")
    .json(&new_item)
    .send()
    .await?;

// POST with form body
let resp = client
    .post("https://api.example.com/login")
    .form(&[("username", "admin"), ("password", "secret")])
    .send()
    .await?;

// Custom headers and query params
let resp = client
    .get("https://api.example.com/search")
    .query(&[("q", "rust"), ("page", "1")])
    .header(
        http::header::ACCEPT,
        http::HeaderValue::from_static("application/json"),
    )
    .send()
    .await?;
```

### Reading responses

```rust,ignore
let resp = client.get("https://example.com").send().await?;

// Check status
let status = resp.status();

// Read headers
let content_type = resp.headers().get("content-type");
let length = resp.content_length();

// Consume body (pick one -- each consumes the response)
let text = resp.text().await?;
let bytes = resp.bytes().await?;
let data: MyStruct = resp.json().await?;
```

### Streaming responses

```rust,ignore
let resp = client.get("https://example.com/large-file").send().await?;
let mut stream = resp.stream();

while let Some(chunk) = stream.next().await {
    let bytes = chunk?;
    // process bytes...
}
```

### Error handling

Use `error_for_status()` to convert non-2xx responses into errors:

```rust,ignore
let data: MyStruct = client
    .get("https://api.example.com/data")
    .send()
    .await?
    .error_for_status()?
    .json()
    .await?;
```

### Per-request overrides

```rust,ignore
use std::time::Duration;

let resp = client
    .get("https://slow-api.example.com/data")
    .timeout(Duration::from_secs(60))
    .max_retries(5)
    .send()
    .await?;
```

## Retry Behaviour

When `max_retries > 0`, the client retries on:

- HTTP 429 (Too Many Requests) -- honours `Retry-After` header
- HTTP 502 (Bad Gateway)
- HTTP 503 (Service Unavailable)
- Connection errors and timeouts

Backoff is exponential: `retry_backoff_ms * 2^attempt`. The `Retry-After` header
(integer seconds or RFC 2822 date) is honoured when present, capped at 60 seconds.

After exhausting retries, the last retryable HTTP response is returned (not an error),
so callers can inspect the status and body.

## Authentication

Two convenience methods on `RequestBuilder`:

- `bearer_token(token)` -- sets `Authorization: Bearer {token}`
- `basic_auth(username, password)` -- sets `Authorization: Basic {base64}`

For other schemes, use `header()` directly.
