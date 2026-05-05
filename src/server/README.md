# modo::server

HTTP server startup, host-based routing, and graceful shutdown for modo v0.11.

## Overview

The module provides:

- `Config` — bind address and shutdown timeout, loaded from the `server` YAML section.
- `http(router, config)` — binds a TCP port, starts serving on a background task, and
  returns an `HttpServer` handle. Accepts any `impl Into<axum::Router>`.
- `HttpServer` — opaque handle to the running server; implements the `Task` trait, so it
  integrates directly with the `modo::run!` macro for coordinated, signal-driven shutdown.
- `HostRouter` — routes requests to different axum routers based on the `Host` header,
  supporting exact matches and single-level wildcard subdomains with an optional fallback.
  Implements `Into<axum::Router>` so it can be passed directly to `http()`.
- `MatchedHost` — axum extractor that provides the subdomain (and pattern) captured by a
  wildcard pattern match; available as `MatchedHost` or `Option<MatchedHost>`.

## Path normalization

Trailing slashes are stripped from request paths before routing, so `/app` and
`/app/` resolve to the same handler. The root path `/` is preserved. This is
implemented by wrapping the router with `tower_http::normalize_path::NormalizePathLayer::trim_trailing_slash()`
inside `http()`; it cannot be disabled.

## Usage

### Minimal server with loaded config

Matches the `lib.rs` quick-start. `modo::config::load()` reads
`config/development.yaml` (or whichever file matches `APP_ENV`), and
`config.server` is the `modo::server::Config` consumed by `http()`.

```rust,no_run
use modo::{Config, Result};
use modo::axum::{Router, routing::get};

async fn hello() -> &'static str { "Hello, modo!" }

#[tokio::main]
async fn main() -> Result<()> {
    let config: Config = modo::config::load("config/")?;
    let app = Router::new().route("/", get(hello));
    let server = modo::server::http(app, &config.server).await?;
    modo::run!(server).await
}
```

### Standalone with explicit defaults

```rust,no_run
use modo::server::{Config, http};

#[tokio::main]
async fn main() -> modo::Result<()> {
    let config = Config::default();
    let router = modo::axum::Router::new();
    let server = http(router, &config).await?;
    modo::run!(server).await
}
```

### Host-based routing

`HostRouter` dispatches requests to different routers based on the `Host` header.
Exact hosts take priority over wildcards, and an optional fallback catches unmatched
hosts.

```rust,no_run
use modo::server::{self, Config, HostRouter};

#[tokio::main]
async fn main() -> modo::Result<()> {
    let config = Config::default();

    let app = HostRouter::new()
        .host("acme.com", modo::axum::Router::new())         // exact match
        .host("app.acme.com", modo::axum::Router::new())     // exact match
        .host("*.acme.com", modo::axum::Router::new())       // wildcard subdomain
        .fallback(modo::axum::Router::new());                 // unmatched hosts

    // HostRouter implements Into<axum::Router>, so it can be passed directly to http()
    let server = server::http(app, &config).await?;
    modo::run!(server).await
}
```

### Extracting the matched subdomain

When a request matches a wildcard pattern, `MatchedHost` is available as an axum
extractor. Use `Option<MatchedHost>` for handlers that serve both exact and wildcard
routes.

```rust,no_run
use modo::server::MatchedHost;

async fn handler(matched: MatchedHost) -> String {
    // For a request to "tenant1.acme.com" matching "*.acme.com":
    //   matched.subdomain == "tenant1"
    //   matched.pattern   == "*.acme.com"
    format!("Hello, {}!", matched.subdomain)
}

async fn optional_handler(matched: Option<MatchedHost>) -> String {
    match matched {
        Some(m) => format!("tenant: {}", m.subdomain),
        None => "apex host".to_string(),
    }
}
```

### Loading config from YAML

```rust,no_run
fn main() -> modo::Result<()> {
    // Reads config/development.yaml (or the file named after APP_ENV)
    let config: modo::Config = modo::config::load("config/")?;
    // config.server is a modo::server::Config
    let _server_cfg: &modo::server::Config = &config.server;
    Ok(())
}
```

### Coordinated shutdown with multiple services

`modo::run!` accepts any number of `Task` values and shuts them down in order
after a `SIGTERM` or `Ctrl-C` signal is received.

```rust,no_run
use modo::server::{Config, http};

#[tokio::main]
async fn main() -> modo::Result<()> {
    let cfg = Config::default();
    let router = modo::axum::Router::new();
    let server = http(router, &cfg).await?;
    // Pass additional Task values (workers, schedulers, etc.) as extra arguments.
    // Example: modo::run!(server, worker, scheduler).await
    modo::run!(server).await
}
```

## Configuration

`Config` maps to the `server` section of the application YAML file. All fields are
optional and fall back to their defaults when omitted.

| Field                   | Type     | Default       | Description                                         |
| ----------------------- | -------- | ------------- | --------------------------------------------------- |
| `host`                  | `String` | `"localhost"` | Network interface to bind                           |
| `port`                  | `u16`    | `8080`        | TCP port to listen on                               |
| `shutdown_timeout_secs` | `u64`    | `30`          | Seconds to wait for in-flight requests during drain |

YAML example:

```yaml
server:
  host: 0.0.0.0
  port: ${PORT:8080}
  shutdown_timeout_secs: 30
```

## Key types

| Item          | Kind     | Description                                                                        |
| ------------- | -------- | ---------------------------------------------------------------------------------- |
| `Config`      | struct   | Server bind address and shutdown timeout; deserializes from YAML                   |
| `HttpServer`  | struct   | Opaque handle to the running server; implements `Task` for graceful shutdown       |
| `http`        | async fn | Binds a TCP listener and starts serving; accepts `impl Into<axum::Router>`         |
| `HostRouter`  | struct   | Routes requests to different routers by `Host` header; exact and wildcard matching |
| `MatchedHost` | struct   | Axum extractor providing the subdomain captured by a wildcard `HostRouter` pattern |

## Host resolution

`HostRouter` resolves the effective host from incoming requests by checking headers in
this order:

1. `Forwarded` header (RFC 7239) -- `host=` directive
2. `X-Forwarded-Host` header
3. `Host` header

The resolved value is lowercased and any trailing port is stripped before matching.
