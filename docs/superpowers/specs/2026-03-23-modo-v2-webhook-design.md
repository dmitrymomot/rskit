# Plan 15 — Webhook Delivery

## Overview

Single-shot webhook sender following the [Standard Webhooks](https://www.standardwebhooks.com/) specification. HMAC-SHA256 signing with `webhook-id` / `webhook-timestamp` / `webhook-signature` headers. No built-in retries — the app's job system handles durability and backoff.

Feature-gated under `webhooks`.

## Standard Webhooks Protocol

Three HTTP headers on every request:

| Header | Value |
|---|---|
| `webhook-id` | Caller-provided message ID (e.g. `msg_01JQX...`) |
| `webhook-timestamp` | Unix timestamp in seconds |
| `webhook-signature` | `v1,<base64>` — space-delimited if multiple secrets |

Signed content: `{webhook-id}.{webhook-timestamp}.{raw_body}`

Example request:

```http
POST /webhooks/orders HTTP/1.1
Content-Type: application/json
User-Agent: modo-webhooks/0.1.0
webhook-id: msg_2KWPBgLlAfxdpx2AI54pPJ85f4W
webhook-timestamp: 1711187200
webhook-signature: v1,K5oZfzN95Z3mnVs9k9MHXHEfFMpKb0hJP2mJGtyFWW4=

{"event":"order.completed","data":{"order_id":"ord_01JQXYZ","amount":4999,"currency":"usd"}}
```

Consumers verify using off-the-shelf `standardwebhooks` libraries in Go, TypeScript, Python, PHP, Java, Ruby, C#.

## Design Decisions

| Decision | Choice | Rationale |
|---|---|---|
| Retry logic | None in sender | Job system provides crash-durable retries with backoff |
| Spec | Standard Webhooks | Industry standard (Svix, Zapier, Twilio, Mux, ngrok, Supabase, Kong). Libraries in every language |
| Feature gate | `webhooks` | Pulls in hyper client deps not every app needs |
| Message ID | Caller-provided | Enables idempotency — same job retry sends same ID |
| Secret format | `WebhookSecret` newtype | Parses `whsec_<base64>`, redacted Debug/Display |
| HTTP client | Injected trait | Testable with mocks, app can share connection pool |
| Return type | `Result<WebhookResponse>` | Status + body — caller decides retry strategy |
| Timeout | Configurable, 30s default | Set once at construction |
| User-Agent | Configurable, `modo-webhooks/{version}` default | Override from app config |
| Signing API | Two layers | Low-level HMAC primitives + high-level header-aware helpers |

## Module Structure

```
src/webhook/
  mod.rs          — pub use re-exports
  secret.rs       — WebhookSecret newtype
  signature.rs    — sign/verify + sign_headers/verify_headers + SignedHeaders
  sender.rs       — WebhookSender<C>
  client.rs       — HttpClient trait, WebhookResponse, HyperClient
```

## Types

### `WebhookSecret`

```rust
// src/webhook/secret.rs

pub struct WebhookSecret { key: Vec<u8> }

impl WebhookSecret {
    /// Construct from raw bytes.
    pub fn new(raw: impl Into<Vec<u8>>) -> Self;

    /// Generate a new secret (24 random bytes).
    pub fn generate() -> Self;

    /// Access raw key bytes for HMAC operations.
    pub fn as_bytes(&self) -> &[u8];
}

impl FromStr for WebhookSecret {
    // Parses "whsec_<standard-base64>" — strips prefix, decodes base64.
    // Returns Error::bad_request if prefix missing or base64 invalid.
}

impl Display for WebhookSecret {
    // Serializes as "whsec_<base64>".
}

impl Debug for WebhookSecret {
    // "WebhookSecret(***)" — redacted.
}

impl Deserialize for WebhookSecret {
    // Delegates to FromStr — works in YAML config.
}
```

### `HttpClient` trait and `WebhookResponse`

```rust
// src/webhook/client.rs

pub trait HttpClient: Send + Sync + 'static {
    fn post(
        &self,
        url: &str,
        headers: HeaderMap,
        body: Bytes,
    ) -> impl Future<Output = Result<WebhookResponse>> + Send;
}

pub struct WebhookResponse {
    pub status: StatusCode,
    pub body: Bytes,
}

pub struct HyperClient { /* hyper_util Client internally */ }

impl HyperClient {
    pub fn new(timeout: Duration) -> Self;
}

impl HttpClient for HyperClient {
    // HTTPS via hyper-rustls + webpki-roots.
    // Timeout applied via tokio::time::timeout.
}
```

`HttpClient` is RPITIT — not object-safe, used as concrete type parameter.

### `WebhookSender<C>`

```rust
// src/webhook/sender.rs

struct WebhookSenderInner<C: HttpClient> {
    client: C,
    timeout: Duration,
    user_agent: String,
}

pub struct WebhookSender<C: HttpClient> {
    inner: Arc<WebhookSenderInner<C>>,
}

impl<C: HttpClient> Clone for WebhookSender<C> { /* Arc clone */ }

impl<C: HttpClient> WebhookSender<C> {
    /// Create with explicit client and timeout.
    pub fn new(client: C, timeout: Duration) -> Self;

    /// Override the default user-agent string.
    pub fn with_user_agent(self, user_agent: impl Into<String>) -> Self;

    /// Send a webhook. Steps:
    /// 1. Get current unix timestamp
    /// 2. sign_headers(secrets, id, timestamp, body)
    /// 3. POST to url with content-type, user-agent, and signed headers
    /// 4. Return WebhookResponse or Error on timeout/network failure
    pub async fn send(
        &self,
        url: &str,
        id: &str,
        body: &[u8],
        secrets: &[&WebhookSecret],
    ) -> Result<WebhookResponse>;
}

impl WebhookSender<HyperClient> {
    /// Convenience: default HyperClient with 30s timeout.
    pub fn default_client() -> Self;
}
```

## Signing API

### Low-level primitives

```rust
// src/webhook/signature.rs

/// HMAC-SHA256 of arbitrary content, returned as standard base64.
pub fn sign(secret: &WebhookSecret, content: &[u8]) -> String;

/// Constant-time verify: decodes base64 signature, compares HMAC.
pub fn verify(secret: &WebhookSecret, content: &[u8], signature: &str) -> bool;
```

### High-level header-aware helpers

```rust
// src/webhook/signature.rs

pub struct SignedHeaders {
    pub webhook_id: String,
    pub webhook_timestamp: i64,
    pub webhook_signature: String,  // "v1,<base64>" space-delimited
}

/// Assembles "{id}.{timestamp}.{body}", signs with each secret.
/// Returns headers ready to set on the request.
pub fn sign_headers(
    secrets: &[&WebhookSecret],
    id: &str,
    timestamp: i64,
    body: &[u8],
) -> SignedHeaders;

/// Parses webhook-id/timestamp/signature from HeaderMap,
/// reassembles signed content, tries each v1 signature against secret,
/// validates timestamp within tolerance.
/// Returns Ok(()) or descriptive Error.
pub fn verify_headers(
    secret: &WebhookSecret,
    headers: &http::HeaderMap,
    body: &[u8],
    tolerance: Duration,
) -> Result<()>;
```

## Feature Gate

In `Cargo.toml`:

```toml
[features]
webhooks = ["dep:hmac", "dep:hyper", "dep:hyper-rustls", "dep:hyper-util", "dep:http-body-util"]
```

In `lib.rs`:

```rust
#[cfg(feature = "webhooks")]
pub(crate) mod webhook;

#[cfg(feature = "webhooks")]
pub use webhook::{
    WebhookSecret, WebhookSender, WebhookResponse,
    HyperClient, HttpClient, SignedHeaders,
    sign, verify, sign_headers, verify_headers,
};
```

## Usage Example

### Sending (in a job handler)

```rust
async fn send_webhook_job(
    Service(sender): Service<WebhookSender<HyperClient>>,
    Payload(event): Payload<WebhookEvent>,
) -> modo::Result<()> {
    let body = serde_json::to_vec(&event.payload)?;
    let secret = event.secret.parse::<WebhookSecret>()?;

    let response = sender.send(
        &event.url,
        &event.id,       // same ID on retry = consumer can deduplicate
        &body,
        &[&secret],
    ).await?;

    match response.status.as_u16() {
        200..=299 => Ok(()),
        410       => Ok(()),  // Gone — endpoint removed, stop retrying
        _         => Err(Error::internal("webhook delivery failed")),
    }
}
```

### Verifying (receiving side)

```rust
async fn receive_webhook(
    headers: HeaderMap,
    body: Bytes,
) -> modo::Result<()> {
    let secret = WebhookSecret::new(b"my-secret-bytes");

    verify_headers(&secret, &headers, &body, Duration::from_secs(300))?;

    // Signature valid, process the event...
    Ok(())
}
```

### Low-level signing

```rust
let secret = WebhookSecret::generate();
let content = b"arbitrary data";

let sig = sign(&secret, content);
assert!(verify(&secret, content, &sig));
```

## Testing Strategy

All tests behind `#![cfg(feature = "webhooks")]`.

### Unit tests

- `WebhookSecret`: parse valid `whsec_` string, reject invalid prefix, reject bad base64, round-trip Display/FromStr, Debug is redacted, generate produces valid secret, Deserialize from string
- `sign` / `verify`: known test vector, wrong secret fails, tampered content fails, empty content works
- `sign_headers`: correct header format, multiple secrets produce space-delimited signatures, timestamp matches input
- `verify_headers`: valid signature passes, wrong signature fails, expired timestamp rejected, future timestamp rejected, missing headers return descriptive errors, multi-signature rotation (one valid + one invalid = pass)
- `WebhookSender::send`: mock `HttpClient`, verify correct headers set, verify timeout error, verify user-agent header (default and custom)

### Integration tests

- `HyperClient`: POST to a local test server (tokio TcpListener), verify request arrives with correct headers and body, verify timeout triggers on slow server
- End-to-end: `WebhookSender<HyperClient>` → local server → `verify_headers` on received request
