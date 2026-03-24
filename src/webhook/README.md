# modo::webhook

Outbound webhook delivery following the [Standard Webhooks](https://www.standardwebhooks.com/) specification.

Requires feature `"webhooks"`.

## Key Types

| Item               | Kind   | Description                                                                           |
| ------------------ | ------ | ------------------------------------------------------------------------------------- |
| `WebhookSender<C>` | struct | Signs and delivers webhook payloads via HTTP POST. Clone-cheap (`Arc` inside).        |
| `WebhookSecret`    | struct | HMAC-SHA256 signing key. Serialized as `whsec_<base64>`. `Debug` output is redacted.  |
| `WebhookResponse`  | struct | HTTP status code and body bytes returned by the endpoint.                             |
| `SignedHeaders`    | struct | The three Standard Webhooks request headers produced by `sign_headers`.               |
| `HttpClient`       | trait  | HTTP transport abstraction. Not object-safe; use a concrete type parameter.           |
| `HyperClient`      | struct | Default hyper/rustls HTTP client with a configurable per-request timeout.             |
| `sign`             | fn     | Compute a raw HMAC-SHA256 signature (base64-encoded).                                 |
| `verify`           | fn     | Verify a raw HMAC-SHA256 signature with constant-time comparison.                     |
| `sign_headers`     | fn     | Build the three Standard Webhooks headers from id, timestamp, body, and secrets.      |
| `verify_headers`   | fn     | Verify an incoming request's Standard Webhooks headers with replay-attack protection. |

## Usage

### Sending a Webhook

```rust
use modo::webhook::{WebhookSender, WebhookSecret};

async fn example() -> modo::Result<()> {
    let sender = WebhookSender::default_client(); // 30-second timeout
    let secret: WebhookSecret = "whsec_dGVzdC1rZXktYnl0ZXM=".parse()?;

    let response = sender.send(
        "https://example.com/webhooks",
        "msg_01HXYZ",
        b"{\"event\":\"user.created\"}",
        &[&secret],
    ).await?;

    println!("endpoint returned {}", response.status);
    Ok(())
}
```

### Storing and Loading Secrets

`WebhookSecret` serializes as a `whsec_<base64>` string and implements `FromStr`:

```rust
use modo::webhook::WebhookSecret;

fn roundtrip() -> modo::Result<()> {
    let secret = WebhookSecret::generate();       // 24 random bytes
    let stored = secret.to_string();              // "whsec_<base64>"
    let loaded: WebhookSecret = stored.parse()?; // round-trip via FromStr
    assert_eq!(loaded.as_bytes(), secret.as_bytes());
    Ok(())
}
```

### Key Rotation

Pass multiple secrets to `send` — each produces a `v1,<sig>` entry in the
`webhook-signature` header. A receiver accepts the message if any entry matches:

```rust
use modo::webhook::{WebhookSender, WebhookSecret};

async fn rotate(sender: &WebhookSender<modo::webhook::HyperClient>) -> modo::Result<()> {
    let old: WebhookSecret = "whsec_b2xkLWtleS1ieXRlcw==".parse()?;
    let new: WebhookSecret = "whsec_bmV3LWtleS1ieXRlcw==".parse()?;

    sender.send(
        "https://example.com/webhooks",
        "msg_02HABC",
        b"{\"event\":\"order.paid\"}",
        &[&old, &new],
    ).await?;
    Ok(())
}
```

### Verifying Incoming Webhooks

`verify_headers` reads `webhook-id`, `webhook-timestamp`, and `webhook-signature`
from the request headers, enforces a replay-attack tolerance window, and performs
a constant-time signature check:

```rust
use std::time::Duration;
use modo::webhook::{WebhookSecret, verify_headers};

fn handle_incoming(
    headers: &http::HeaderMap,
    body: &[u8],
    secret: &WebhookSecret,
) -> modo::Result<()> {
    verify_headers(&[secret], headers, body, Duration::from_secs(300))
}
```

### Custom HTTP Client

Implement `HttpClient` to swap in a different transport (e.g. for testing or to
add retry logic):

```rust
use bytes::Bytes;
use http::HeaderMap;
use modo::webhook::{HttpClient, WebhookResponse, WebhookSender};

struct LoggingClient;

impl HttpClient for LoggingClient {
    async fn post(
        &self,
        url: &str,
        _headers: HeaderMap,
        _body: Bytes,
    ) -> modo::Result<WebhookResponse> {
        tracing::info!(url, "webhook delivered");
        Ok(WebhookResponse {
            status: http::StatusCode::OK,
            body: Bytes::new(),
        })
    }
}

let sender = WebhookSender::new(LoggingClient);
```

## Configuration

Store secrets as `whsec_<base64>` strings in YAML config. `WebhookSecret` implements
`serde::Deserialize` and parses the `whsec_` prefix automatically:

```yaml
webhooks:
    endpoint_secret: "whsec_dGVzdC1rZXktYnl0ZXM="
```

```rust
use modo::webhook::WebhookSecret;

#[derive(serde::Deserialize)]
struct WebhooksConfig {
    endpoint_secret: WebhookSecret,
}
```
