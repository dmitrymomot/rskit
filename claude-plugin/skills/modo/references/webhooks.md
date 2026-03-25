# Webhooks

Outbound webhook delivery with Standard Webhooks signing. Feature-gated under `webhooks`.

```toml
# Cargo.toml
modo = { path = "..", features = ["webhooks"] }
```

All types are re-exported from the crate root under `#[cfg(feature = "webhooks")]`:

```rust
use modo::{
    HttpClient, HyperClient, SignedHeaders,
    WebhookResponse, WebhookSecret, WebhookSender,
};
```

Source: `src/webhook/` (mod.rs, client.rs, sender.rs, secret.rs, signature.rs).

Free functions `sign`, `verify`, `sign_headers`, `verify_headers` are exported from the `webhook` module but **not** re-exported at the crate root. Access them via:

```rust
use modo::webhook::{sign, verify, sign_headers, verify_headers};
```

---

## HttpClient Trait

Defines how a single HTTP POST is sent. Uses RPITIT (not object-safe).

```rust
pub trait HttpClient: Send + Sync + 'static {
    fn post(
        &self,
        url: &str,
        headers: HeaderMap,
        body: Bytes,
    ) -> impl Future<Output = Result<WebhookResponse>> + Send;
}
```

Because `HttpClient` uses return-position `impl Trait`, it cannot be used as `dyn HttpClient`. Always use a concrete type parameter: `WebhookSender<HyperClient>` or `WebhookSender<MyClient>`.

---

## HyperClient

Default `HttpClient` implementation backed by `hyper` + `hyper-rustls` with TLS (webpki roots).

```rust
let client = HyperClient::new(Duration::from_secs(30));
```

- Constructor takes a `timeout: Duration`. Requests exceeding this timeout return an error.
- Uses `hyper_rustls::HttpsConnectorBuilder` with `.with_webpki_roots()`, `.https_or_http()`, `.enable_http1()`.
- Timeout enforced via `tokio::time::timeout`.

---

## WebhookSender\<C\>

High-level sender that signs payloads per the Standard Webhooks protocol and delivers them via an `HttpClient`. Clone-cheap (`Arc<Inner>` pattern).

### Construction

```rust
// With default HyperClient (30s timeout):
let sender = WebhookSender::default_client();

// With custom client:
let sender = WebhookSender::new(my_client);

// Override User-Agent (must be called before cloning):
let sender = WebhookSender::default_client()
    .with_user_agent("my-app/2.0");
```

- `default_client()` is an inherent method on `WebhookSender<HyperClient>` only.
- Default User-Agent: `modo-webhooks/<version>`.
- `with_user_agent()` panics if called after the sender has been cloned (uses `Arc::get_mut`).

### Sending

```rust
let response = sender.send(url, id, body, secrets).await?;
```

Signature:

```rust
pub async fn send(
    &self,
    url: &str,       // endpoint to POST to
    id: &str,        // unique message ID for idempotency (e.g. "msg_<ulid>")
    body: &[u8],     // raw request body (typically JSON)
    secrets: &[&WebhookSecret], // one or more signing secrets
) -> Result<WebhookResponse>
```

Behavior:
- Validates that `secrets` is non-empty and `id` is non-empty.
- Validates `url` parses as `http::Uri`.
- Gets current UTC timestamp.
- Calls `sign_headers()` to produce Standard Webhooks headers.
- Sets headers: `content-type: application/json`, `user-agent`, `webhook-id`, `webhook-timestamp`, `webhook-signature`.
- Delegates to `self.inner.client.post()`.
- Empty body is accepted.

---

## WebhookResponse

```rust
pub struct WebhookResponse {
    pub status: StatusCode,
    pub body: Bytes,
}
```

Returned by `HttpClient::post()` and `WebhookSender::send()`. The caller decides what status codes constitute success or failure.

---

## WebhookSecret

A webhook signing secret stored as raw bytes. Serialized as `whsec_<base64>` strings.

```rust
// Generate a new 24-byte random secret:
let secret = WebhookSecret::generate();

// From raw bytes:
let secret = WebhookSecret::new(b"my-key".to_vec());

// Parse from whsec_ string:
let secret: WebhookSecret = "whsec_dGVzdA==".parse()?;

// Access raw bytes:
let bytes = secret.as_bytes();

// Display (always whsec_ prefixed):
println!("{}", secret); // whsec_<base64>
```

- `Debug` output is always redacted: `WebhookSecret(***)`.
- Implements `Serialize` / `Deserialize` as `whsec_<base64>` strings (safe for YAML/JSON config).
- `FromStr` requires the `whsec_` prefix; returns `Error::bad_request` if missing or if base64 is invalid.
- Uses standard base64 encoding (`base64::engine::general_purpose::STANDARD`).

---

## Standard Webhooks Signing

### SignedHeaders

```rust
pub struct SignedHeaders {
    pub webhook_id: String,        // message ID
    pub webhook_timestamp: i64,    // Unix seconds
    pub webhook_signature: String, // "v1,<base64>" entries, space-separated
}
```

Multiple secrets produce multiple `v1,<base64>` entries in `webhook_signature`, separated by spaces. This supports key rotation: the receiver accepts the message if any entry matches.

### sign_headers

```rust
pub fn sign_headers(
    secrets: &[&WebhookSecret],
    id: &str,
    timestamp: i64,
    body: &[u8],
) -> SignedHeaders
```

Builds signed content as `{id}.{timestamp}.{body}` and signs with each secret using HMAC-SHA256. Panics if `secrets` is empty (caller must validate).

### sign / verify (low-level)

```rust
// HMAC-SHA256, returns standard base64 string:
pub fn sign(secret: &WebhookSecret, content: &[u8]) -> String

// Constant-time verification, returns false on invalid base64 or mismatch:
pub fn verify(secret: &WebhookSecret, content: &[u8], signature: &str) -> bool
```

### verify_headers (inbound verification)

```rust
pub fn verify_headers(
    secrets: &[&WebhookSecret],
    headers: &http::HeaderMap,
    body: &[u8],
    tolerance: Duration,
) -> Result<()>
```

For verifying incoming webhooks:
- Reads `webhook-id`, `webhook-timestamp`, `webhook-signature` from headers.
- Checks timestamp is within `tolerance` of current time (replay-attack protection).
- Tries every `v1,` signature entry against every secret. Returns `Ok(())` on first match.
- Returns `Error::bad_request` if no signature matches, headers are missing, or timestamp is outside tolerance.

---

## Gotchas

- **base64 crate, not encoding::base64url**: The webhook module uses the `base64` crate with `general_purpose::STANDARD` encoding. This is standard base64 with padding, per the Standard Webhooks spec. Do NOT use `modo::encoding::base64url` (which is RFC 4648 no-padding).
- **HttpClient is not object-safe**: Uses RPITIT. Always use concrete type parameters, never `Arc<dyn HttpClient>`.
- **with_user_agent panics after clone**: Must be called on the original `WebhookSender` before any `.clone()` calls.
- **sign_headers panics on empty secrets**: `WebhookSender::send()` validates this before calling, but direct callers of `sign_headers()` must ensure non-empty.
- **No retry logic**: `WebhookSender` sends once. Retry/backoff is the caller's responsibility (e.g., via the job queue).
- **No redirect following**: `HyperClient` does not follow redirects.
- **Content-Type is always `application/json`**: Hardcoded in `WebhookSender::send()`.
