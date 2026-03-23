# Webhook Delivery Implementation Plan (Plan 15)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add single-shot webhook sender following the Standard Webhooks spec — HMAC-SHA256 signing, `webhook-id/timestamp/signature` headers, injectable HTTP client trait, no built-in retries.

**Architecture:** Two API layers: low-level `sign()`/`verify()` HMAC primitives and high-level `sign_headers()`/`verify_headers()` that handle the full Standard Webhooks protocol. `WebhookSender<C: HttpClient>` wraps an injected HTTP client for testability. `HyperClient` is the default implementation using hyper+rustls. `WebhookSecret` newtype handles `whsec_<base64>` parsing/serialization. All feature-gated under `webhooks`.

**Tech Stack:** `hmac` + `sha2` (HMAC-SHA256), `base64` crate (standard base64), hyper 1.x + hyper-rustls + hyper-util (HTTP client), modo's existing error/service patterns.

**Spec:** `docs/superpowers/specs/2026-03-23-modo-v2-webhook-design.md`

**Reference implementations:** `src/storage/signing.rs` (HMAC pattern), `src/storage/client.rs` (hyper client pattern), `src/encoding/base64url.rs` (encoding pattern)

---

### Task 1: Add `base64` crate and `webhooks` feature gate

**Files:**
- Modify: `Cargo.toml`

- [ ] **Step 1: Add `base64` dependency and `webhooks` feature**

In `Cargo.toml`, add `base64` to the optional dependencies section (after the `http-body-util` line):

```toml
base64 = { version = "0.22", optional = true }
```

Add the `webhooks` feature to the `[features]` section (after the `storage-test` line):

```toml
webhooks = ["dep:hmac", "dep:base64", "dep:hyper", "dep:hyper-rustls", "dep:hyper-util", "dep:http-body-util"]
webhooks-test = ["webhooks"]
```

Update the `full` feature to include `"webhooks"`:

```toml
full = ["templates", "sse", "auth", "sentry", "email", "storage", "webhooks"]
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check --features webhooks`
Expected: compiles with no errors.

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml
git commit -m "feat(webhook): add webhooks feature gate and base64 dependency"
```

---

### Task 2: Create `WebhookSecret` newtype

**Files:**
- Create: `src/webhook/mod.rs`
- Create: `src/webhook/secret.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Create module scaffold**

Create `src/webhook/mod.rs`:

```rust
mod secret;

pub use secret::WebhookSecret;
```

Create `src/webhook/secret.rs` with the `WebhookSecret` struct (empty for now, just enough for the test):

```rust
use std::fmt;
use std::str::FromStr;

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

const PREFIX: &str = "whsec_";

pub struct WebhookSecret {
    key: Vec<u8>,
}

impl WebhookSecret {
    /// Construct from raw bytes.
    pub fn new(raw: impl Into<Vec<u8>>) -> Self {
        Self { key: raw.into() }
    }

    /// Generate a new secret with 24 random bytes.
    pub fn generate() -> Self {
        let mut key = vec![0u8; 24];
        rand::fill(&mut key[..]);
        Self { key }
    }

    /// Access raw key bytes for HMAC operations.
    pub fn as_bytes(&self) -> &[u8] {
        &self.key
    }
}

impl FromStr for WebhookSecret {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        let encoded = s
            .strip_prefix(PREFIX)
            .ok_or_else(|| Error::bad_request("webhook secret must start with 'whsec_'"))?;
        let key = BASE64
            .decode(encoded)
            .map_err(|e| Error::bad_request(format!("invalid base64 in webhook secret: {e}")))?;
        Ok(Self { key })
    }
}

impl fmt::Display for WebhookSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", PREFIX, BASE64.encode(&self.key))
    }
}

impl fmt::Debug for WebhookSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("WebhookSecret(***)")
    }
}

impl Serialize for WebhookSecret {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for WebhookSecret {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_valid_whsec_string() {
        let raw = vec![1u8, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];
        let encoded = format!("whsec_{}", BASE64.encode(&raw));
        let secret: WebhookSecret = encoded.parse().unwrap();
        assert_eq!(secret.as_bytes(), &raw);
    }

    #[test]
    fn reject_missing_prefix() {
        let result = "notwhsec_AQIDBA==".parse::<WebhookSecret>();
        assert!(result.is_err());
        assert!(result.err().unwrap().message().contains("whsec_"));
    }

    #[test]
    fn reject_invalid_base64() {
        let result = "whsec_!!!invalid!!!".parse::<WebhookSecret>();
        assert!(result.is_err());
        assert!(result.err().unwrap().message().contains("base64"));
    }

    #[test]
    fn display_roundtrip() {
        let secret = WebhookSecret::new(vec![10, 20, 30, 40]);
        let displayed = secret.to_string();
        assert!(displayed.starts_with("whsec_"));
        let parsed: WebhookSecret = displayed.parse().unwrap();
        assert_eq!(parsed.as_bytes(), secret.as_bytes());
    }

    #[test]
    fn debug_is_redacted() {
        let secret = WebhookSecret::new(vec![1, 2, 3]);
        let debug = format!("{secret:?}");
        assert_eq!(debug, "WebhookSecret(***)");
        assert!(!debug.contains("1"));
    }

    #[test]
    fn generate_produces_valid_secret() {
        let secret = WebhookSecret::generate();
        assert_eq!(secret.as_bytes().len(), 24);
        // Round-trip through Display/FromStr
        let displayed = secret.to_string();
        let parsed: WebhookSecret = displayed.parse().unwrap();
        assert_eq!(parsed.as_bytes(), secret.as_bytes());
    }

    #[test]
    fn serialize_roundtrip() {
        let secret = WebhookSecret::new(vec![5, 10, 15, 20]);
        let json = serde_json::to_string(&secret).unwrap();
        let parsed: WebhookSecret = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.as_bytes(), secret.as_bytes());
    }

    #[test]
    fn deserialize_from_string() {
        let raw = vec![99u8; 16];
        let whsec = format!("\"whsec_{}\"", BASE64.encode(&raw));
        let secret: WebhookSecret = serde_json::from_str(&whsec).unwrap();
        assert_eq!(secret.as_bytes(), &raw);
    }
}
```

- [ ] **Step 2: Wire module into lib.rs**

Add to `src/lib.rs` (after the `storage` block):

```rust
#[cfg(feature = "webhooks")]
pub mod webhook;
```

- [ ] **Step 3: Run tests**

Run: `cargo test --features webhooks webhook::secret`
Expected: all 7 tests pass.

- [ ] **Step 4: Run clippy**

Run: `cargo clippy --features webhooks --tests -- -D warnings`
Expected: no warnings.

- [ ] **Step 5: Commit**

```bash
git add src/webhook/mod.rs src/webhook/secret.rs src/lib.rs
git commit -m "feat(webhook): add WebhookSecret newtype with whsec_ parsing"
```

---

### Task 3: Low-level signing primitives (`sign` / `verify`)

**Files:**
- Create: `src/webhook/signature.rs`
- Modify: `src/webhook/mod.rs`

- [ ] **Step 1: Create `src/webhook/signature.rs` with sign/verify and tests**

```rust
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use hmac::{Hmac, Mac};
use sha2::Sha256;

use super::secret::WebhookSecret;

type HmacSha256 = Hmac<Sha256>;

/// HMAC-SHA256 of arbitrary content, returned as standard base64.
pub fn sign(secret: &WebhookSecret, content: &[u8]) -> String {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .expect("HMAC accepts any key length");
    mac.update(content);
    BASE64.encode(mac.finalize().into_bytes())
}

/// Constant-time verify via `hmac::Mac::verify_slice`.
/// Decodes the base64 signature and compares against HMAC-SHA256 of content.
pub fn verify(secret: &WebhookSecret, content: &[u8], signature: &str) -> bool {
    let sig_bytes = match BASE64.decode(signature) {
        Ok(b) => b,
        Err(_) => return false,
    };
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .expect("HMAC accepts any key length");
    mac.update(content);
    mac.verify_slice(&sig_bytes).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_produces_base64() {
        let secret = WebhookSecret::new(b"test-key".to_vec());
        let sig = sign(&secret, b"hello");
        // Should be valid base64
        assert!(BASE64.decode(&sig).is_ok());
    }

    #[test]
    fn verify_valid_signature() {
        let secret = WebhookSecret::new(b"test-key".to_vec());
        let sig = sign(&secret, b"hello");
        assert!(verify(&secret, b"hello", &sig));
    }

    #[test]
    fn verify_wrong_secret_fails() {
        let secret1 = WebhookSecret::new(b"key-one".to_vec());
        let secret2 = WebhookSecret::new(b"key-two".to_vec());
        let sig = sign(&secret1, b"hello");
        assert!(!verify(&secret2, b"hello", &sig));
    }

    #[test]
    fn verify_tampered_content_fails() {
        let secret = WebhookSecret::new(b"test-key".to_vec());
        let sig = sign(&secret, b"hello");
        assert!(!verify(&secret, b"tampered", &sig));
    }

    #[test]
    fn verify_invalid_base64_returns_false() {
        let secret = WebhookSecret::new(b"test-key".to_vec());
        assert!(!verify(&secret, b"hello", "!!!not-base64!!!"));
    }

    #[test]
    fn sign_empty_content() {
        let secret = WebhookSecret::new(b"test-key".to_vec());
        let sig = sign(&secret, b"");
        assert!(verify(&secret, b"", &sig));
    }

    #[test]
    fn known_test_vector() {
        // Precomputed: HMAC-SHA256("test-secret", "test-content") as base64
        let secret = WebhookSecret::new(b"test-secret".to_vec());
        let sig = sign(&secret, b"test-content");
        // Verify round-trip; the exact value is deterministic
        assert!(verify(&secret, b"test-content", &sig));
        // Different content must fail
        assert!(!verify(&secret, b"other-content", &sig));
    }
}
```

- [ ] **Step 2: Update `src/webhook/mod.rs` to export**

```rust
mod secret;
mod signature;

pub use secret::WebhookSecret;
pub use signature::{sign, verify};
```

- [ ] **Step 3: Run tests**

Run: `cargo test --features webhooks webhook::signature`
Expected: all 7 tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/webhook/signature.rs src/webhook/mod.rs
git commit -m "feat(webhook): add low-level sign/verify HMAC-SHA256 primitives"
```

---

### Task 4: High-level `sign_headers` / `verify_headers`

**Files:**
- Modify: `src/webhook/signature.rs`

- [ ] **Step 1: Add `SignedHeaders`, `sign_headers`, `verify_headers` to `src/webhook/signature.rs`**

Add after the `verify` function:

```rust
use std::time::Duration;

use crate::error::{Error, Result};

/// Headers produced by signing a webhook payload.
pub struct SignedHeaders {
    pub webhook_id: String,
    pub webhook_timestamp: i64,
    pub webhook_signature: String,
}

/// Assemble Standard Webhooks signed content and sign with each secret.
/// Returns headers ready to set on the request.
///
/// # Panics
/// Panics if `secrets` is empty. Callers must validate before calling.
pub fn sign_headers(
    secrets: &[&WebhookSecret],
    id: &str,
    timestamp: i64,
    body: &[u8],
) -> SignedHeaders {
    assert!(!secrets.is_empty(), "at least one secret required");

    let content = build_signed_content(id, timestamp, body);
    let sigs: Vec<String> = secrets
        .iter()
        .map(|s| format!("v1,{}", sign(s, &content)))
        .collect();

    SignedHeaders {
        webhook_id: id.to_string(),
        webhook_timestamp: timestamp,
        webhook_signature: sigs.join(" "),
    }
}

/// Parse Standard Webhooks headers and verify the signature.
/// Tries each `v1,` signature in the header against each secret.
/// Validates timestamp is within `tolerance` of now.
pub fn verify_headers(
    secrets: &[&WebhookSecret],
    headers: &http::HeaderMap,
    body: &[u8],
    tolerance: Duration,
) -> Result<()> {
    let id = header_str(headers, "webhook-id")?;
    let ts_str = header_str(headers, "webhook-timestamp")?;
    let sig_header = header_str(headers, "webhook-signature")?;

    let timestamp: i64 = ts_str
        .parse()
        .map_err(|_| Error::bad_request("invalid webhook-timestamp"))?;

    // Check timestamp tolerance
    let now = chrono::Utc::now().timestamp();
    let diff = (now - timestamp).unsigned_abs();
    if diff > tolerance.as_secs() {
        return Err(Error::bad_request("webhook timestamp outside tolerance"));
    }

    let content = build_signed_content(id, timestamp, body);

    // Try each v1 signature against each secret
    for sig_entry in sig_header.split(' ') {
        let raw_sig = match sig_entry.strip_prefix("v1,") {
            Some(s) => s,
            None => continue, // skip non-v1 signatures
        };
        for secret in secrets {
            if verify(secret, &content, raw_sig) {
                return Ok(());
            }
        }
    }

    Err(Error::bad_request("no valid webhook signature found"))
}

fn build_signed_content(id: &str, timestamp: i64, body: &[u8]) -> Vec<u8> {
    let prefix = format!("{id}.{timestamp}.");
    let mut content = Vec::with_capacity(prefix.len() + body.len());
    content.extend_from_slice(prefix.as_bytes());
    content.extend_from_slice(body);
    content
}

fn header_str<'a>(headers: &'a http::HeaderMap, name: &str) -> Result<&'a str> {
    headers
        .get(name)
        .ok_or_else(|| Error::bad_request(format!("missing {name} header")))?
        .to_str()
        .map_err(|_| Error::bad_request(format!("invalid {name} header encoding")))
}
```

- [ ] **Step 2: Add tests for `sign_headers` and `verify_headers`**

Add to the `#[cfg(test)] mod tests` block in `src/webhook/signature.rs`:

```rust
    use std::time::Duration;

    fn make_headers(id: &str, ts: i64, sig: &str) -> http::HeaderMap {
        let mut headers = http::HeaderMap::new();
        headers.insert("webhook-id", id.parse().unwrap());
        headers.insert("webhook-timestamp", ts.to_string().parse().unwrap());
        headers.insert("webhook-signature", sig.parse().unwrap());
        headers
    }

    #[test]
    fn sign_headers_single_secret() {
        let secret = WebhookSecret::new(b"key".to_vec());
        let sh = sign_headers(&[&secret], "msg_123", 1000, b"body");
        assert_eq!(sh.webhook_id, "msg_123");
        assert_eq!(sh.webhook_timestamp, 1000);
        assert!(sh.webhook_signature.starts_with("v1,"));
        assert!(!sh.webhook_signature.contains(' '));
    }

    #[test]
    fn sign_headers_multiple_secrets() {
        let s1 = WebhookSecret::new(b"key1".to_vec());
        let s2 = WebhookSecret::new(b"key2".to_vec());
        let sh = sign_headers(&[&s1, &s2], "msg_123", 1000, b"body");
        let parts: Vec<&str> = sh.webhook_signature.split(' ').collect();
        assert_eq!(parts.len(), 2);
        assert!(parts[0].starts_with("v1,"));
        assert!(parts[1].starts_with("v1,"));
        assert_ne!(parts[0], parts[1]);
    }

    #[test]
    #[should_panic(expected = "at least one secret")]
    fn sign_headers_empty_secrets_panics() {
        sign_headers(&[], "msg_123", 1000, b"body");
    }

    #[test]
    fn verify_headers_valid() {
        let secret = WebhookSecret::new(b"key".to_vec());
        let now = chrono::Utc::now().timestamp();
        let sh = sign_headers(&[&secret], "msg_1", now, b"payload");
        let headers = make_headers(&sh.webhook_id, sh.webhook_timestamp, &sh.webhook_signature);
        let result = verify_headers(&[&secret], &headers, b"payload", Duration::from_secs(300));
        assert!(result.is_ok());
    }

    #[test]
    fn verify_headers_wrong_secret_fails() {
        let sign_secret = WebhookSecret::new(b"sign-key".to_vec());
        let verify_secret = WebhookSecret::new(b"wrong-key".to_vec());
        let now = chrono::Utc::now().timestamp();
        let sh = sign_headers(&[&sign_secret], "msg_1", now, b"data");
        let headers = make_headers(&sh.webhook_id, sh.webhook_timestamp, &sh.webhook_signature);
        let result = verify_headers(&[&verify_secret], &headers, b"data", Duration::from_secs(300));
        assert!(result.is_err());
    }

    #[test]
    fn verify_headers_expired_timestamp() {
        let secret = WebhookSecret::new(b"key".to_vec());
        let old_ts = chrono::Utc::now().timestamp() - 600; // 10 minutes ago
        let sh = sign_headers(&[&secret], "msg_1", old_ts, b"data");
        let headers = make_headers(&sh.webhook_id, sh.webhook_timestamp, &sh.webhook_signature);
        let result = verify_headers(&[&secret], &headers, b"data", Duration::from_secs(300));
        assert!(result.is_err());
        assert!(result.err().unwrap().message().contains("tolerance"));
    }

    #[test]
    fn verify_headers_future_timestamp() {
        let secret = WebhookSecret::new(b"key".to_vec());
        let future_ts = chrono::Utc::now().timestamp() + 600; // 10 minutes ahead
        let sh = sign_headers(&[&secret], "msg_1", future_ts, b"data");
        let headers = make_headers(&sh.webhook_id, sh.webhook_timestamp, &sh.webhook_signature);
        let result = verify_headers(&[&secret], &headers, b"data", Duration::from_secs(300));
        assert!(result.is_err());
    }

    #[test]
    fn verify_headers_missing_header() {
        let secret = WebhookSecret::new(b"key".to_vec());
        let headers = http::HeaderMap::new(); // empty
        let result = verify_headers(&[&secret], &headers, b"data", Duration::from_secs(300));
        assert!(result.is_err());
        assert!(result.err().unwrap().message().contains("missing"));
    }

    #[test]
    fn verify_headers_multi_signature_rotation() {
        let old_secret = WebhookSecret::new(b"old-key".to_vec());
        let new_secret = WebhookSecret::new(b"new-key".to_vec());
        let now = chrono::Utc::now().timestamp();
        // Sign with both secrets (key rotation)
        let sh = sign_headers(&[&old_secret, &new_secret], "msg_1", now, b"data");
        let headers = make_headers(&sh.webhook_id, sh.webhook_timestamp, &sh.webhook_signature);
        // Verify with only the new secret — should still pass (one signature matches)
        let result = verify_headers(&[&new_secret], &headers, b"data", Duration::from_secs(300));
        assert!(result.is_ok());
    }

    #[test]
    fn verify_headers_multi_secret_on_verify_side() {
        let secret = WebhookSecret::new(b"the-key".to_vec());
        let wrong_secret = WebhookSecret::new(b"wrong-key".to_vec());
        let now = chrono::Utc::now().timestamp();
        // Sign with one secret
        let sh = sign_headers(&[&secret], "msg_1", now, b"data");
        let headers = make_headers(&sh.webhook_id, sh.webhook_timestamp, &sh.webhook_signature);
        // Verify with both (wrong + correct) — should pass because one matches
        let result = verify_headers(&[&wrong_secret, &secret], &headers, b"data", Duration::from_secs(300));
        assert!(result.is_ok());
    }
```

- [ ] **Step 3: Update `src/webhook/mod.rs` exports**

```rust
mod secret;
mod signature;

pub use secret::WebhookSecret;
pub use signature::{sign, verify, sign_headers, verify_headers, SignedHeaders};
```

- [ ] **Step 4: Run tests**

Run: `cargo test --features webhooks webhook::signature`
Expected: all tests pass (7 low-level + 9 high-level = 16 total).

- [ ] **Step 5: Run clippy**

Run: `cargo clippy --features webhooks --tests -- -D warnings`
Expected: no warnings.

- [ ] **Step 6: Commit**

```bash
git add src/webhook/signature.rs src/webhook/mod.rs
git commit -m "feat(webhook): add Standard Webhooks sign_headers/verify_headers"
```

---

### Task 5: `HttpClient` trait, `WebhookResponse`, and `HyperClient`

**Files:**
- Create: `src/webhook/client.rs`
- Modify: `src/webhook/mod.rs`

- [ ] **Step 1: Create `src/webhook/client.rs`**

```rust
use std::time::Duration;

use bytes::Bytes;
use http::HeaderMap;
use http::StatusCode;
use http_body_util::{BodyExt, Full};
use hyper_rustls::HttpsConnectorBuilder;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;

use crate::error::{Error, Result};

/// Response from a webhook delivery attempt.
pub struct WebhookResponse {
    pub status: StatusCode,
    pub body: Bytes,
}

/// Trait for sending webhook HTTP POST requests.
/// RPITIT — not object-safe, used as concrete type parameter.
pub trait HttpClient: Send + Sync + 'static {
    fn post(
        &self,
        url: &str,
        headers: HeaderMap,
        body: Bytes,
    ) -> impl Future<Output = Result<WebhookResponse>> + Send;
}

/// Default hyper-based HTTP client with TLS support.
pub struct HyperClient {
    client: Client<
        hyper_rustls::HttpsConnector<hyper_util::client::legacy::connect::HttpConnector>,
        Full<Bytes>,
    >,
    timeout: Duration,
}

impl HyperClient {
    pub fn new(timeout: Duration) -> Self {
        let connector = HttpsConnectorBuilder::new()
            .with_webpki_roots()
            .https_or_http()
            .enable_http1()
            .build();
        let client = Client::builder(TokioExecutor::new()).build(connector);
        Self { client, timeout }
    }
}

impl HttpClient for HyperClient {
    async fn post(
        &self,
        url: &str,
        headers: HeaderMap,
        body: Bytes,
    ) -> Result<WebhookResponse> {
        let uri: http::Uri = url
            .parse()
            .map_err(|e| Error::bad_request(format!("invalid webhook url: {e}")))?;

        let mut builder = hyper::Request::builder()
            .method(hyper::Method::POST)
            .uri(uri);

        for (name, value) in &headers {
            builder = builder.header(name, value);
        }

        let request = builder
            .body(Full::new(body))
            .map_err(|e| Error::internal(format!("failed to build webhook request: {e}")))?;

        let response = tokio::time::timeout(self.timeout, self.client.request(request))
            .await
            .map_err(|_| Error::internal("webhook request timed out"))?
            .map_err(|e| Error::internal(format!("webhook request failed: {e}")))?;

        let status = response.status();
        let response_body = response
            .into_body()
            .collect()
            .await
            .map_err(|e| Error::internal(format!("failed to read webhook response: {e}")))?
            .to_bytes();

        Ok(WebhookResponse {
            status,
            body: response_body,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hyper_client_creates_without_panic() {
        let _ = HyperClient::new(Duration::from_secs(30));
    }
}
```

- [ ] **Step 2: Update `src/webhook/mod.rs`**

```rust
mod client;
mod secret;
mod signature;

pub use client::{HttpClient, HyperClient, WebhookResponse};
pub use secret::WebhookSecret;
pub use signature::{sign, sign_headers, verify, verify_headers, SignedHeaders};
```

- [ ] **Step 3: Run tests and clippy**

Run: `cargo test --features webhooks webhook::client`
Run: `cargo clippy --features webhooks --tests -- -D warnings`
Expected: both pass cleanly.

- [ ] **Step 4: Commit**

```bash
git add src/webhook/client.rs src/webhook/mod.rs
git commit -m "feat(webhook): add HttpClient trait and HyperClient implementation"
```

---

### Task 6: `WebhookSender<C>`

**Files:**
- Create: `src/webhook/sender.rs`
- Modify: `src/webhook/mod.rs`

- [ ] **Step 1: Create `src/webhook/sender.rs`**

```rust
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use http::HeaderMap;

use super::client::{HttpClient, HyperClient, WebhookResponse};
use super::secret::WebhookSecret;
use super::signature::sign_headers;
use crate::error::{Error, Result};

struct WebhookSenderInner<C: HttpClient> {
    client: C,
    user_agent: String,
}

pub struct WebhookSender<C: HttpClient> {
    inner: Arc<WebhookSenderInner<C>>,
}

impl<C: HttpClient> Clone for WebhookSender<C> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<C: HttpClient> WebhookSender<C> {
    /// Create a new sender with the given HTTP client.
    pub fn new(client: C) -> Self {
        Self {
            inner: Arc::new(WebhookSenderInner {
                client,
                user_agent: format!("modo-webhooks/{}", env!("CARGO_PKG_VERSION")),
            }),
        }
    }

    /// Override the default user-agent string.
    pub fn with_user_agent(mut self, user_agent: impl Into<String>) -> Self {
        let inner = Arc::get_mut(&mut self.inner)
            .expect("with_user_agent must be called before cloning");
        inner.user_agent = user_agent.into();
        self
    }

    /// Send a webhook following the Standard Webhooks protocol.
    ///
    /// - `url`: the endpoint to POST to
    /// - `id`: unique message ID for idempotency (e.g. `msg_<ulid>`)
    /// - `body`: raw request body (typically JSON)
    /// - `secrets`: one or more signing secrets (supports key rotation)
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
        // Validate URL early — it comes from user/app input
        let _: http::Uri = url
            .parse()
            .map_err(|e| Error::bad_request(format!("invalid webhook url: {e}")))?;

        let timestamp = chrono::Utc::now().timestamp();
        let signed = sign_headers(secrets, id, timestamp, body);

        let mut headers = HeaderMap::new();
        headers.insert("content-type", "application/json".parse().unwrap());
        headers.insert(
            "user-agent",
            self.inner.user_agent.parse().unwrap(),
        );
        headers.insert("webhook-id", signed.webhook_id.parse().map_err(
            |_| Error::bad_request("webhook id contains invalid header characters"),
        )?);
        headers.insert(
            "webhook-timestamp",
            signed.webhook_timestamp.to_string().parse().unwrap(),
        );
        headers.insert("webhook-signature", signed.webhook_signature.parse().map_err(
            |_| Error::internal("generated invalid webhook-signature header"),
        )?);

        self.inner
            .client
            .post(url, headers, Bytes::copy_from_slice(body))
            .await
    }
}

impl WebhookSender<HyperClient> {
    /// Convenience constructor with default HyperClient (30s timeout).
    pub fn default_client() -> Self {
        Self::new(HyperClient::new(Duration::from_secs(30)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::StatusCode;

    struct MockClient {
        response_status: StatusCode,
        captured_headers: std::sync::Mutex<Option<HeaderMap>>,
        captured_body: std::sync::Mutex<Option<Bytes>>,
    }

    impl MockClient {
        fn new(status: StatusCode) -> Self {
            Self {
                response_status: status,
                captured_headers: std::sync::Mutex::new(None),
                captured_body: std::sync::Mutex::new(None),
            }
        }

        fn captured_headers(&self) -> HeaderMap {
            self.captured_headers.lock().unwrap().clone().unwrap()
        }

        fn captured_body(&self) -> Bytes {
            self.captured_body.lock().unwrap().clone().unwrap()
        }
    }

    impl HttpClient for MockClient {
        async fn post(
            &self,
            _url: &str,
            headers: HeaderMap,
            body: Bytes,
        ) -> Result<WebhookResponse> {
            *self.captured_headers.lock().unwrap() = Some(headers);
            *self.captured_body.lock().unwrap() = Some(body);
            Ok(WebhookResponse {
                status: self.response_status,
                body: Bytes::new(),
            })
        }
    }

    #[tokio::test]
    async fn send_sets_correct_headers() {
        let mock = MockClient::new(StatusCode::OK);
        let sender = WebhookSender::new(mock);
        let secret = WebhookSecret::new(b"test-key".to_vec());

        let result = sender
            .send("http://example.com/hook", "msg_123", b"{}", &[&secret])
            .await;
        assert!(result.is_ok());

        let headers = sender.inner.client.captured_headers();
        assert_eq!(headers.get("content-type").unwrap(), "application/json");
        assert_eq!(headers.get("webhook-id").unwrap(), "msg_123");
        assert!(headers.get("webhook-timestamp").is_some());
        assert!(headers
            .get("webhook-signature")
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("v1,"));
    }

    #[tokio::test]
    async fn send_default_user_agent() {
        let mock = MockClient::new(StatusCode::OK);
        let sender = WebhookSender::new(mock);
        let secret = WebhookSecret::new(b"key".to_vec());

        sender
            .send("http://example.com/hook", "msg_1", b"{}", &[&secret])
            .await
            .unwrap();

        let headers = sender.inner.client.captured_headers();
        let ua = headers.get("user-agent").unwrap().to_str().unwrap();
        assert!(ua.starts_with("modo-webhooks/"));
    }

    #[tokio::test]
    async fn send_custom_user_agent() {
        let mock = MockClient::new(StatusCode::OK);
        let sender = WebhookSender::new(mock).with_user_agent("my-app/2.0");
        let secret = WebhookSecret::new(b"key".to_vec());

        sender
            .send("http://example.com/hook", "msg_1", b"{}", &[&secret])
            .await
            .unwrap();

        let headers = sender.inner.client.captured_headers();
        assert_eq!(headers.get("user-agent").unwrap(), "my-app/2.0");
    }

    #[tokio::test]
    async fn send_empty_secrets_rejected() {
        let mock = MockClient::new(StatusCode::OK);
        let sender = WebhookSender::new(mock);

        let result = sender
            .send("http://example.com/hook", "msg_1", b"{}", &[])
            .await;
        assert!(result.is_err());
        assert!(result.err().unwrap().message().contains("secret"));
    }

    #[tokio::test]
    async fn send_empty_id_rejected() {
        let mock = MockClient::new(StatusCode::OK);
        let sender = WebhookSender::new(mock);
        let secret = WebhookSecret::new(b"key".to_vec());

        let result = sender
            .send("http://example.com/hook", "", b"{}", &[&secret])
            .await;
        assert!(result.is_err());
        assert!(result.err().unwrap().message().contains("id"));
    }

    #[tokio::test]
    async fn send_empty_body_accepted() {
        let mock = MockClient::new(StatusCode::OK);
        let sender = WebhookSender::new(mock);
        let secret = WebhookSecret::new(b"key".to_vec());

        let result = sender
            .send("http://example.com/hook", "msg_1", b"", &[&secret])
            .await;
        assert!(result.is_ok());
        assert!(sender.inner.client.captured_body().is_empty());
    }

    #[tokio::test]
    async fn send_returns_response_status() {
        let mock = MockClient::new(StatusCode::GONE);
        let sender = WebhookSender::new(mock);
        let secret = WebhookSecret::new(b"key".to_vec());

        let response = sender
            .send("http://example.com/hook", "msg_1", b"{}", &[&secret])
            .await
            .unwrap();
        assert_eq!(response.status, StatusCode::GONE);
    }

    #[tokio::test]
    async fn send_invalid_url_rejected() {
        let mock = MockClient::new(StatusCode::OK);
        let sender = WebhookSender::new(mock);
        let secret = WebhookSecret::new(b"key".to_vec());

        let result = sender
            .send("not a valid url", "msg_1", b"{}", &[&secret])
            .await;
        assert!(result.is_err());
        assert!(result.err().unwrap().message().contains("invalid webhook url"));
    }
}
```

- [ ] **Step 2: Update `src/webhook/mod.rs`**

```rust
mod client;
mod secret;
mod sender;
mod signature;

pub use client::{HttpClient, HyperClient, WebhookResponse};
pub use secret::WebhookSecret;
pub use sender::WebhookSender;
pub use signature::{sign, sign_headers, verify, verify_headers, SignedHeaders};
```

- [ ] **Step 3: Run tests**

Run: `cargo test --features webhooks webhook::sender`
Expected: all 8 tests pass.

- [ ] **Step 4: Run clippy**

Run: `cargo clippy --features webhooks --tests -- -D warnings`
Expected: no warnings.

- [ ] **Step 5: Commit**

```bash
git add src/webhook/sender.rs src/webhook/mod.rs
git commit -m "feat(webhook): add WebhookSender with mock-based unit tests"
```

---

### Task 7: Wire re-exports in `lib.rs` and run full test suite

**Files:**
- Modify: `src/lib.rs`

- [ ] **Step 1: Add re-exports to `src/lib.rs`**

Add after the existing `storage` re-export block:

```rust
#[cfg(feature = "webhooks")]
pub use webhook::{
    HttpClient, HyperClient, SignedHeaders, WebhookResponse, WebhookSecret, WebhookSender,
};
```

Note: `sign`, `verify`, `sign_headers`, `verify_headers` stay namespaced under `modo::webhook::` — not re-exported at crate root.

- [ ] **Step 2: Run full test suite**

Run: `cargo test --features webhooks`
Expected: all webhook tests pass alongside existing tests.

- [ ] **Step 3: Run full clippy**

Run: `cargo clippy --features webhooks --tests -- -D warnings`
Expected: no warnings.

- [ ] **Step 4: Run format check**

Run: `cargo fmt --check`
Expected: no formatting issues.

- [ ] **Step 5: Commit**

```bash
git add src/lib.rs
git commit -m "feat(webhook): wire re-exports in lib.rs"
```

---

### Task 8: Integration tests — HyperClient with local server

**Files:**
- Create: `tests/webhook_integration.rs`

- [ ] **Step 1: Create integration test file**

```rust
#![cfg(feature = "webhooks")]

use std::time::Duration;

use bytes::Bytes;
use http::StatusCode;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use modo::webhook::{
    sign_headers, verify_headers, HyperClient, HttpClient, WebhookSecret, WebhookSender,
};

/// Start a minimal HTTP server that captures the request and returns the given status.
async fn start_test_server(
    response_status: u16,
) -> (String, tokio::task::JoinHandle<(String, Vec<u8>)>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let url = format!("http://127.0.0.1:{}", addr.port());

    let handle = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        // Single read is safe for these small test payloads (< 8KB)
        let mut buf = vec![0u8; 8192];
        let n = stream.read(&mut buf).await.unwrap();
        buf.truncate(n);
        let raw = String::from_utf8_lossy(&buf).to_string();

        let response = format!(
            "HTTP/1.1 {response_status} OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        );
        stream.write_all(response.as_bytes()).await.unwrap();
        stream.shutdown().await.unwrap();

        (raw, buf)
    });

    (url, handle)
}

#[tokio::test]
async fn hyper_client_post_reaches_server() {
    let (url, handle) = start_test_server(200).await;
    let client = HyperClient::new(Duration::from_secs(5));

    let mut headers = http::HeaderMap::new();
    headers.insert("content-type", "application/json".parse().unwrap());
    headers.insert("x-test", "hello".parse().unwrap());

    let response = client
        .post(&url, headers, Bytes::from_static(b"test-body"))
        .await
        .unwrap();

    assert_eq!(response.status, StatusCode::OK);

    let (raw_request, _) = handle.await.unwrap();
    assert!(raw_request.contains("POST / HTTP/1.1"));
    assert!(raw_request.contains("x-test: hello"));
    assert!(raw_request.contains("test-body"));
}

#[tokio::test]
async fn hyper_client_timeout_on_slow_server() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let url = format!("http://127.0.0.1:{}", addr.port());

    // Server accepts but never responds
    let _handle = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        tokio::time::sleep(Duration::from_secs(60)).await;
        drop(stream);
    });

    let client = HyperClient::new(Duration::from_millis(100));
    let result = client
        .post(&url, http::HeaderMap::new(), Bytes::new())
        .await;

    assert!(result.is_err());
    assert!(result.err().unwrap().message().contains("timed out"));
}

#[tokio::test]
async fn end_to_end_send_and_verify() {
    let (url, handle) = start_test_server(200).await;
    let sender = WebhookSender::new(HyperClient::new(Duration::from_secs(5)));
    let secret = WebhookSecret::new(b"e2e-test-secret".to_vec());

    let body = b"{\"event\":\"test\"}";
    let response = sender
        .send(&url, "msg_e2e_1", body, &[&secret])
        .await
        .unwrap();
    assert_eq!(response.status, StatusCode::OK);

    let (raw_request, _) = handle.await.unwrap();

    // Parse headers from raw HTTP request for round-trip verification
    let mut received_headers = http::HeaderMap::new();
    let header_section = raw_request.split("\r\n\r\n").next().unwrap();
    for line in header_section.lines().skip(1) {
        // skip request line
        if let Some((name, value)) = line.split_once(": ") {
            received_headers.insert(
                http::header::HeaderName::from_bytes(name.as_bytes()).unwrap(),
                value.parse().unwrap(),
            );
        }
    }

    // Verify the received request can be validated with verify_headers
    verify_headers(
        &[&secret],
        &received_headers,
        body,
        Duration::from_secs(300),
    )
    .unwrap();
}
```

- [ ] **Step 2: Run integration tests**

Run: `cargo test --features webhooks --test webhook_integration`
Expected: all 3 integration tests pass.

- [ ] **Step 3: Commit**

```bash
git add tests/webhook_integration.rs
git commit -m "test(webhook): add integration tests with local HTTP server"
```

---

### Task 9: Update CLAUDE.md and final verification

**Files:**
- Modify: `CLAUDE.md`

- [ ] **Step 1: Update CLAUDE.md**

Update "Current Work" section — change Plan 15 line to mark it as DONE:

```
- **Plan 15 (Webhook Delivery):** DONE — `src/webhook/` module with `WebhookSender<C>`, `HttpClient` trait, `HyperClient`, `WebhookSecret`, Standard Webhooks signing. Feature-gated under `webhooks`
```

Add to the "Gotchas > Dependencies" section:

```
- `base64` crate for standard base64 (webhooks feature) — NOT `encoding::base64url` which is RFC 4648 no-padding
```

- [ ] **Step 2: Run full test suite with all features**

Run: `cargo test --features full`
Run: `cargo clippy --features full --tests -- -D warnings`
Run: `cargo fmt --check`
Expected: all pass cleanly.

- [ ] **Step 3: Commit**

```bash
git add CLAUDE.md
git commit -m "docs: update CLAUDE.md with webhook module gotchas"
```
