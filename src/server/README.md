# server

HTTP server startup and graceful shutdown for the modo framework.

## Overview

The module exposes two items:

- `Config` — bind address and shutdown timeout, loaded from the `server` YAML section.
- `http(router, config)` — binds a TCP port, starts serving on a background task, and
  returns an `HttpServer` handle.

`HttpServer` implements the `Task` trait, so it integrates directly with the `run!` macro
for coordinated, signal-driven shutdown.

## Usage

### Minimal server

```rust
use modo::server::{Config, http};
use modo::run;

#[tokio::main]
async fn main() -> modo::Result<()> {
    let config = Config::default();
    let router = modo::axum::Router::new();
    let server = http(router, &config).await?;
    run!(server).await
}
```

### Loading config from YAML

```rust
use modo::server::Config;
use modo::config::load;

let config: modo::Config = load("config/").unwrap();
// config.server is a server::Config
```

### Coordinated shutdown with multiple services

`run!` accepts any number of `Task` values and shuts them down in order after a
`SIGTERM` or `Ctrl-C` signal is received.

```rust
use modo::server::{Config, http};
use modo::run;

#[tokio::main]
async fn main() -> modo::Result<()> {
    let cfg = Config::default();
    let router = modo::axum::Router::new();
    let server = http(router, &cfg).await?;
    // pass additional Task values (workers, schedulers, etc.) as extra arguments
    run!(server).await
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

## Key Types

| Symbol       | Description                                                                  |
| ------------ | ---------------------------------------------------------------------------- |
| `Config`     | Server bind address and shutdown timeout; deserializes from YAML             |
| `HttpServer` | Opaque handle to the running server; implements `Task` for graceful shutdown |
| `http`       | `async fn http(router: axum::Router, config: &Config) -> Result<HttpServer>` |
