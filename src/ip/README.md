# modo::ip

Client IP address resolution for reverse-proxy deployments.

`ClientIpLayer` is a Tower middleware that inspects `X-Forwarded-For` and
`X-Real-IP` headers, applies an optional trusted-proxy allowlist to prevent
spoofing, and stores the resolved address as a `ClientIp` extension on every
request. Handlers read it with the `ClientIp` axum extractor.

## Key Types

| Type                  | Description                                                  |
| --------------------- | ------------------------------------------------------------ |
| [`ClientIpLayer`]     | Tower layer; add to the router with `.layer()`               |
| [`ClientIp`]          | Axum extractor; wraps `std::net::IpAddr`                     |
| [`extract_client_ip`] | Low-level resolution function (headers + trusted proxies)    |

Canonical paths: `modo::ip::{ClientIp, ClientIpLayer}`.

The richer [`ClientInfo`](../client) extractor — IP, user-agent, parsed device
fields, and a server-computed fingerprint — lives in `modo::client`.

Re-exported flat indexes:

- `modo::prelude::ClientIp` — in the handler-ambient prelude
- `modo::extractors::ClientIp` — flat extractor index
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

For IP plus user-agent, parsed device fields, and a server-computed
fingerprint, use [`ClientInfo`](../client) from `modo::client` — see that
module's README for usage.

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

`ClientIpLayer` must run **before** any middleware that depends on the
resolved IP — notably [`SessionLayer`](../auth/session) (alias for
`CookieSessionLayer`), which reads `ClientIp` for fingerprint validation. In
axum, the outermost layer runs first, so `ClientIpLayer` must be the last
`.layer(..)` call on the router.

```text
Router::new()
    .route(..)
    .layer(session_layer)          // inner — sees ClientIp
    .layer(ClientIpLayer::new());  // outer — resolves IP first
```

## Resolution function

For non-Tower contexts (e.g., extracting a client IP from a raw
[`HeaderMap`](http::HeaderMap) inside a job handler), call
[`extract_client_ip`] directly:

```rust
use std::net::IpAddr;
use modo::ip::extract_client_ip;

let proxies: Vec<ipnet::IpNet> = vec!["10.0.0.0/8".parse().unwrap()];
let headers = http::HeaderMap::new();
let connect_ip: Option<IpAddr> = None;
let ip = extract_client_ip(&headers, &proxies, connect_ip);
```
