# modo::client

Client-context types — IP, user-agent, parsed device fields, and a server-computed
browser fingerprint — shared across HTTP, audit, and session code paths.

## Key types

| Type / function       | Description                                                                 |
| --------------------- | --------------------------------------------------------------------------- |
| [`ClientInfo`]        | Axum extractor + builder for client metadata                                |
| [`parse_device_name`] | UA → human-readable device, e.g. `"Chrome on macOS"`                        |
| [`parse_device_type`] | UA → `"desktop"` / `"mobile"` / `"tablet"`                                  |
| [`compute_fingerprint`] | SHA-256 of UA + Accept-Language + Accept-Encoding (64-char hex)           |

Canonical paths: `modo::client::{ClientInfo, parse_device_name, parse_device_type, compute_fingerprint}`.

## Usage

### In a handler

```rust
use modo::client::ClientInfo;

async fn whoami(info: ClientInfo) -> String {
    format!(
        "{} from {}",
        info.device_name_value().unwrap_or("Unknown"),
        info.ip_value().unwrap_or("?"),
    )
}
```

`ClientInfo::ip_value()` returns `None` when `ClientIpLayer` is not applied.
Other fields are populated from request headers and never fail to extract.

### Outside a handler

For background jobs and CLI tools, build a `ClientInfo` manually:

```rust
use modo::client::ClientInfo;

let info = ClientInfo::new()
    .ip("1.2.3.4")
    .user_agent("worker/1.0")
    .device_name("worker on Linux")
    .device_type("desktop")
    .fingerprint("job-runner");
```

### From a `HeaderMap`

When middleware already holds the request headers, use `from_headers` — it
parses device fields from the user-agent and computes the fingerprint:

```rust
let info = ClientInfo::from_headers(
    Some("1.2.3.4".into()),
    headers.get("user-agent").and_then(|v| v.to_str().ok()).unwrap_or(""),
    headers.get("accept-language").and_then(|v| v.to_str().ok()).unwrap_or(""),
    headers.get("accept-encoding").and_then(|v| v.to_str().ok()).unwrap_or(""),
);
```

## Consumers

- [`modo::audit`](../audit) — `AuditEntry::client_info(ClientInfo)` persists
  `ip`, `user_agent`, `device_name`, `device_type`, and `fingerprint` columns
  on each audit row.
- [`modo::auth::session`](../auth/session) — both the cookie and JWT transports
  take a `&ClientInfo` as input to session creation; the fingerprint is used
  for hijack detection (when enabled).
