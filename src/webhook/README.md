# modo::webhook

Outbound webhook delivery following the [Standard Webhooks](https://www.standardwebhooks.com/) specification.

Requires feature `"webhooks"`:

```toml
[dependencies]
modo = { version = "0.5", features = ["webhooks"] }
```

## Key Types

| Item              | Kind   | Description                                                                           |
| ----------------- | ------ | ------------------------------------------------------------------------------------- |
| `WebhookSender`   | struct | Signs and delivers webhook payloads via HTTP POST. Clone-cheap (`Arc` inside).        |
| `WebhookSecret`   | struct | HMAC-SHA256 signing key. Serialized as `whsec_<base64>`. `Debug` output is redacted.  |
| `WebhookResponse` | struct | HTTP status code and body bytes returned by the endpoint.                             |
| `SignedHeaders`   | struct | The three Standard Webhooks request headers produced by `sign_headers`.               |
| `sign`            | fn     | Compute a raw HMAC-SHA256 signature (base64-encoded).                                 |
| `verify`          | fn     | Verify a raw HMAC-SHA256 signature with constant-time comparison.                     |
| `sign_headers`    | fn     | Build the three Standard Webhooks headers from id, timestamp, body, and secrets.      |
| `verify_headers`  | fn     | Verify an incoming request's Standard Webhooks headers with replay-attack protection. |

## Usage

### Sending a Webhook

```rust,ignore
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

### Custom User-Agent

Call `with_user_agent` immediately after construction, before cloning:

```rust,ignore
use modo::webhook::WebhookSender;

let sender = WebhookSender::default_client()
    .with_user_agent("my-app/2.0");
```

### Shared HTTP Client

Pass an existing `modo::http::Client` to share connection pools across modules:

```rust,ignore
use modo::webhook::WebhookSender;

let client = modo::http::Client::builder()
    .timeout(std::time::Duration::from_secs(10))
    .build();
let sender = WebhookSender::new(client);
```

### Storing and Loading Secrets

`WebhookSecret` serializes as a `whsec_<base64>` string and implements `FromStr`:

```rust,ignore
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

Pass multiple secrets to `send` -- each produces a `v1,<sig>` entry in the
`webhook-signature` header. A receiver accepts the message if any entry matches:

```rust,ignore
use modo::webhook::{WebhookSender, WebhookSecret};

async fn rotate(sender: &WebhookSender) -> modo::Result<()> {
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

```rust,ignore
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

## Configuration

Store secrets as `whsec_<base64>` strings in YAML config. `WebhookSecret` implements
`serde::Deserialize` and parses the `whsec_` prefix automatically:

```yaml
webhooks:
    endpoint_secret: "whsec_dGVzdC1rZXktYnl0ZXM="
```

```rust,ignore
use modo::webhook::WebhookSecret;

#[derive(serde::Deserialize)]
struct WebhooksConfig {
    endpoint_secret: WebhookSecret,
}
```

## Error handling

All errors are returned as `modo::Error` with a 400 Bad Request status:

| Condition | Message |
|-----------|---------|
| Empty `secrets` slice passed to `send` | `"at least one secret required"` |
| Empty `id` passed to `send` | `"webhook id must not be empty"` |
| Invalid URL passed to `send` | `"invalid webhook url: ..."` |
| Missing webhook header in `verify_headers` | `"missing <name> header"` |
| Timestamp outside tolerance in `verify_headers` | `"webhook timestamp outside tolerance"` |
| No signature matches in `verify_headers` | `"no valid webhook signature found"` |
| Secret string missing `whsec_` prefix | `"webhook secret must start with 'whsec_'"` |
| Invalid base64 in secret string | `"invalid base64 in webhook secret: ..."` |
