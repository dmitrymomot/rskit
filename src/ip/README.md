# modo::ip

Client IP address resolution for reverse-proxy deployments.

`ClientIpLayer` is a Tower middleware that inspects `X-Forwarded-For` and
`X-Real-IP` headers, applies an optional trusted-proxy allowlist to prevent
spoofing, and stores the resolved address as a `ClientIp` extension on every
request. Handlers read it with the `ClientIp` axum extractor.

## Key Types

| Type                | Description                                                  |
| ------------------- | ------------------------------------------------------------ |
| `ClientIpLayer`     | Tower layer; add to the router with `.layer()`               |
| `ClientIp`          | Axum extractor; wraps `std::net::IpAddr`                     |
| `ClientInfo`        | Axum extractor: IP + user-agent + `x-fingerprint` header     |
| `extract_client_ip` | Low-level resolution function (headers + trusted proxies)    |

Canonical paths: `modo::ip::{ClientIp, ClientInfo, ClientIpLayer}`.

Re-exported flat indexes:

- `modo::prelude::ClientIp` — in the handler-ambient prelude
- `modo::extractors::{ClientIp, ClientInfo}` — flat extractor index
- `modo::middlewares::ClientIp` — alias for `ClientIpLayer` in the layer
  index (call `mw::ClientIp::new()` or `mw::ClientIp::with_trusted_proxies(..)`)

## Usage

### Without trusted proxies

Trust all proxy headers unconditionally. Use this when every request
already passes through a load balancer you control.

```rust
use axum::{Router, routing::get};
use modo::ip::ClientIpLayer;

let app: Router = Router::new()
    .route("/", get(handler))
    .layer(ClientIpLayer::new());

async fn handler(modo::ip::ClientIp(ip): modo::ip::ClientIp) -> String {
    ip.to_string()
}
```

### With trusted proxies

Only trust `X-Forwarded-For` and `X-Real-IP` when the connection originates
from a known CIDR range. Requests from other addresses use the raw socket IP.

```rust
use axum::{Router, routing::get};
use modo::ip::ClientIpLayer;

let proxies: Vec<ipnet::IpNet> = vec![
    "10.0.0.0/8".parse().unwrap(),
    "172.16.0.0/12".parse().unwrap(),
];

let app: Router = Router::new()
    .route("/ip", get(handler))
    .layer(ClientIpLayer::with_trusted_proxies(proxies));

async fn handler(modo::ip::ClientIp(ip): modo::ip::ClientIp) -> String {
    ip.to_string()
}
```

### Extracting full client metadata

`ClientInfo` bundles the resolved IP together with the `User-Agent` header
and an optional `X-Fingerprint` header. Unlike `ClientIp`, it never fails to
extract — missing fields are simply `None`. Use it for audit logs, session
fingerprinting, or analytics.

```rust
use modo::ip::ClientInfo;

async fn handler(info: ClientInfo) -> String {
    format!(
        "ip={:?} ua={:?} fp={:?}",
        info.ip_value(),
        info.user_agent_value(),
        info.fingerprint_value(),
    )
}
```

`ClientInfo::ip_value()` returns `None` if `ClientIpLayer` is not applied.
For non-HTTP call sites (background jobs, CLI tools), build one with the
fluent API:

```rust
use modo::ip::ClientInfo;

let info = ClientInfo::new()
    .ip("1.2.3.4")
    .user_agent("worker/1.0")
    .fingerprint("job-runner");
```

### Loading trusted proxies from config

`modo::Config` exposes a `trusted_proxies: Vec<String>` field (YAML key
`trusted_proxies`). Parse it at startup and pass to `ClientIpLayer`:

```rust
use modo::Config;
use modo::ip::ClientIpLayer;

let config: Config = modo::config::load("config/").unwrap();
let proxies: Vec<ipnet::IpNet> = config
    .trusted_proxies
    .iter()
    .filter_map(|s| s.parse().ok())
    .collect();

let layer = ClientIpLayer::with_trusted_proxies(proxies);
```

Example `config/app.yaml`:

```yaml
trusted_proxies:
    - 10.0.0.0/8
    - 172.16.0.0/12
    - 192.168.0.0/16
```

## IP Resolution Order

1. If `trusted_proxies` is non-empty and the connecting IP is **not** in any
   trusted range, return the connecting IP directly (ignore all headers).
2. `X-Forwarded-For` — leftmost valid IP.
3. `X-Real-IP` — value parsed as an IP address.
4. `ConnectInfo` socket address.
5. `127.0.0.1` as final fallback.

## Ordering with other layers

`ClientIpLayer` must be applied **before** `SessionLayer`. The session
middleware reads the `ClientIp` extension for fingerprint validation.

```no_run
use axum::Router;
use modo::auth::session::SessionLayer;
use modo::ip::ClientIpLayer;

// ClientIpLayer must wrap SessionLayer so IP resolution happens first.
// Apply layers in reverse order: the last .layer() call is the outermost.
let app: Router = Router::new()
    // ...routes...
    .layer(session_layer)         // inner — runs after ClientIpLayer
    .layer(ClientIpLayer::new()); // outer — resolves IP first
```
