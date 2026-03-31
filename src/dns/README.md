# modo::dns

DNS-based domain ownership verification for custom-domain flows.

Provides `DomainVerifier` for checking TXT record ownership and CNAME routing
via raw UDP DNS queries, without depending on the system resolver or any
external DNS library beyond `simple-dns`.

## Feature flag

This module is compiled only when the `dns` feature is enabled.

```toml
[dependencies]
modo = { version = "0.3", features = ["dns"] }
```

## Key types

| Type                            | Purpose                                                                           |
| ------------------------------- | --------------------------------------------------------------------------------- |
| `DnsConfig`                     | Nameserver address, TXT prefix, and timeout                                       |
| `DomainVerifier`                | Performs TXT and CNAME lookups; `Arc`-backed, cheap to clone                      |
| `DomainStatus`                  | Result of `verify_domain` — individual `txt_verified` / `cname_verified` booleans |
| `DnsError`                      | Error variants with stable `"dns:<kind>"` codes                                   |
| `generate_verification_token()` | 13-char base36 token for TXT challenges                                           |

## Configuration

```rust,ignore
use modo::dns::DnsConfig;

// Construct with a nameserver address (defaults: txt_prefix = "_modo-verify", timeout_ms = 5000)
let config = DnsConfig::new("8.8.8.8:53");

// Or override individual fields after construction
let mut config = DnsConfig::new("8.8.8.8");
config.txt_prefix = "_myapp-verify".into();
config.timeout_ms = 3000;
```

The struct also implements `Default` (nameserver `"8.8.8.8"`) and deserializes from
YAML via serde. `txt_prefix` and `timeout_ms` are optional:

```yaml
dns:
    nameserver: "8.8.8.8:53"
    txt_prefix: "_myapp-verify"
    timeout_ms: 3000
```

## Usage

### Step 1 — generate a token and show it to the user

```rust,ignore
use modo::dns::generate_verification_token;

let token = generate_verification_token(); // e.g. "0r9xkbf2a1m4z"
// Instruct the user:
// Add TXT record: _modo-verify.example.com  →  "0r9xkbf2a1m4z"
// Add CNAME:      example.com              →  app.yourservice.com
```

### Step 2 — verify ownership

```rust,no_run
use modo::dns::{DnsConfig, DomainStatus, DomainVerifier};

async fn verify(token: &str) -> modo::Result<()> {
    let config = DnsConfig::new("8.8.8.8:53");
    let verifier = DomainVerifier::from_config(&config)?;

    // Check TXT record: returns true when _modo-verify.example.com TXT == token
    // (case-sensitive). Returns false when absent or mismatched (not an error).
    let txt_ok = verifier.check_txt("example.com", token).await?;

    // Check CNAME: returns true when the target matches
    // (case-insensitive, trailing dot stripped).
    let cname_ok = verifier.check_cname("example.com", "app.yourservice.com").await?;

    // Or check both at once (runs concurrently via tokio::join!):
    let status: DomainStatus = verifier
        .verify_domain("example.com", token, "app.yourservice.com")
        .await?;

    if status.txt_verified && status.cname_verified {
        // domain is fully verified
    }

    Ok(())
}
```

## Error handling

`DnsError` variants are attached to `modo::Error` using `.chain()` and
`.with_code()` so that the error kind survives the response pipeline:

```rust,ignore
use modo::dns::DnsError;

// Before the response is serialized:
if let Some(dns_err) = err.source_as::<DnsError>() {
    // match on dns_err
}

// After the response:
if err.error_code() == Some("dns:timeout") { /* ... */ }
```

| Variant         | Code                 | HTTP status         |
| --------------- | -------------------- | ------------------- |
| `Timeout`       | `dns:timeout`        | 504 Gateway Timeout |
| `ServerFailure` | `dns:server_failure` | 502 Bad Gateway     |
| `Refused`       | `dns:refused`        | 502 Bad Gateway     |
| `Malformed`     | `dns:malformed`      | 502 Bad Gateway     |
| `NetworkError`  | `dns:network_error`  | 502 Bad Gateway     |
| `InvalidInput`  | `dns:invalid_input`  | 400 Bad Request     |

## Integration with modo

Register `DomainVerifier` in the service registry so handlers can extract it:

```rust,ignore
use modo::service::Registry;
use modo::dns::{DnsConfig, DomainVerifier};

let config: DnsConfig = app_config.dns; // from your YAML config
let verifier = DomainVerifier::from_config(&config)?;

let mut registry = Registry::new();
registry.add(verifier);
let state = registry.into_state();

let app = axum::Router::new()
    // .route(...)
    .with_state(state);
```

In a handler, extract the verifier with `Service<DomainVerifier>`:

```rust,ignore
use modo::Service;
use modo::dns::DomainVerifier;

async fn verify_handler(
    Service(verifier): Service<DomainVerifier>,
) -> modo::Result<()> {
    let ok = verifier.check_txt("example.com", "my-token").await?;
    // ...
    Ok(())
}
```
