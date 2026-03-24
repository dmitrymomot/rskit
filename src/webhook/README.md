# modo::webhook

Outbound webhook delivery following the [Standard Webhooks](https://www.standardwebhooks.com/) specification.

Requires feature `"webhooks"`.

## Key Types

| Type               | Description                                                                    |
| ------------------ | ------------------------------------------------------------------------------ |
| `WebhookSender<C>` | Signs and delivers webhook payloads via HTTP POST. Clone-cheap (`Arc` inside). |
| `WebhookSecret`    | HMAC-SHA256 signing key. Serialized as `whsec_<base64>`. `Debug` is redacted.  |
| `WebhookResponse`  | Status code and body returned by the endpoint.                                 |
| `SignedHeaders`    | The three Standard Webhooks request headers.                                   |
| `HttpClient`       | Trait for the HTTP transport layer (not object-safe).                          |
| `HyperClient`      | Default hyper/rustls HTTP client with a configurable timeout.                  |

## Usage

### Sending a Webhook

```rust
use modo::webhook::{WebhookSender, WebhookSecret};

# async fn example() -> modo::Result<()> {
let sender = WebhookSender::default_client(); // 30-second timeout
let secret: WebhookSecret = "whsec_dGVzdC1rZXktYnl0ZXM=".parse()?;

let response = sender.send(
    "https://example.com/webhooks",
    "msg_01HXYZ",
    b"{\"event\":\"user.created\"}",
    &[&secret],
).await?;

println!("endpoint returned {}", response.status);
# Ok(())
# }
```

### Storing and Loading Secrets

```rust
use modo::webhook::WebhookSecret;

let secret = WebhookSecret::generate();       // 24 random bytes
let stored = secret.to_string();              // "whsec_<base64>"
let loaded: WebhookSecret = stored.parse()?; // round-trip
```

### Key Rotation

Pass multiple secrets to `send` — each produces a `v1,<sig>` entry in the
`webhook-signature` header. The receiver accepts any matching entry:

```rust
use modo::webhook::{WebhookSender, WebhookSecret};

# async fn example() -> modo::Result<()> {
let sender = WebhookSender::default_client();
let old: WebhookSecret = "whsec_b2xkLWtleS1ieXRlcw==".parse()?;
let new: WebhookSecret = "whsec_bmV3LWtleS1ieXRlcw==".parse()?;

sender.send("https://example.com/webhooks", "msg_02HABC",
    b"{\"event\":\"order.paid\"}", &[&old, &new]).await?;
# Ok(())
# }
```

### Verifying Incoming Webhooks

```rust
use std::time::Duration;
use modo::webhook::{WebhookSecret, verify_headers};

fn handle_incoming(headers: &http::HeaderMap, body: &[u8], secret: &WebhookSecret)
    -> modo::Result<()>
{
    verify_headers(&[secret], headers, body, Duration::from_secs(300))
}
```

### Custom HTTP Client

```rust
use bytes::Bytes;
use http::HeaderMap;
use modo::webhook::{HttpClient, WebhookResponse, WebhookSender};

struct LoggingClient;

impl HttpClient for LoggingClient {
    async fn post(&self, url: &str, _h: HeaderMap, _b: Bytes)
        -> modo::Result<WebhookResponse>
    {
        tracing::info!(url, "webhook delivered");
        Ok(WebhookResponse { status: http::StatusCode::OK, body: Bytes::new() })
    }
}

let sender = WebhookSender::new(LoggingClient);
```

## Configuration

Store secrets as `whsec_<base64>` strings in YAML. `WebhookSecret` implements
`serde::Deserialize` directly:

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
