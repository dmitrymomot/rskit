# modo::tracing

Tracing initialisation and structured logging for modo applications.

Wraps [`tracing_subscriber`](https://docs.rs/tracing-subscriber) with a simple YAML-driven configuration and a built-in Sentry integration. Call `init` once at startup and hold the returned `TracingGuard` for the process lifetime.

This module handles **subscriber setup** (log format, level, Sentry). For HTTP request/response tracing, see `modo::middleware::tracing()` which creates a `tower_http` `TraceLayer` with `ModoMakeSpan` (defined in `modo::middleware`).

## Key types

| Type / Item                                  | Description                                                                                    |
| -------------------------------------------- | ---------------------------------------------------------------------------------------------- |
| `Config`                                     | Log level, output format, and optional `SentryConfig`. Deserialised from the `tracing:` YAML section. |
| `init(&Config) -> Result<TracingGuard>`      | Initialises the global subscriber and optional Sentry client.                                  |
| `TracingGuard`                               | RAII guard that keeps the subscriber and Sentry client alive. Implements `Task` and `Default`. |
| `SentryConfig`                               | Sentry DSN, environment tag, and `sample_rate` / `traces_sample_rate`.                         |
| `debug!`, `info!`, `warn!`, `error!`, `trace!` | Re-exports of the corresponding `tracing` macros for convenience.                            |

Sentry support is always compiled in — there is no `sentry` feature flag. Sentry is enabled at runtime by supplying a non-empty DSN in `Config::sentry.dsn`.

## Usage

### Basic setup

```rust,no_run
use modo::config::load;
use modo::Config;
use modo::runtime::Task;

#[tokio::main]
async fn main() -> modo::Result<()> {
    let config: Config = load("config/").unwrap();
    let guard = modo::tracing::init(&config.tracing)?;

    // ... run the application ...

    guard.shutdown().await
}
```

### Using with `run!`

`TracingGuard` implements `Task`, so it integrates directly with the `run!` macro for ordered shutdown:

```rust,no_run
use modo::config::load;
use modo::Config;

#[tokio::main]
async fn main() -> modo::Result<()> {
    let config: Config = load("config/").unwrap();
    let guard = modo::tracing::init(&config.tracing)?;

    // ... start server, jobs, etc. ...

    modo::run!(guard).await
}
```

## Configuration

The `tracing` section in your YAML config file maps directly to `Config`:

```yaml
tracing:
    level: info # any RUST_LOG / EnvFilter directive; overridden by RUST_LOG env var
    format: pretty # "pretty" (default) | "json" | compact (any other value)
```

### Log formats

| Value      | Output style                           |
| ---------- | -------------------------------------- |
| `"pretty"` | Human-readable multi-line (default)    |
| `"json"`   | Machine-readable JSON, one object/line |
| other      | Compact single-line                    |

`RUST_LOG` overrides `level` when set.

### With Sentry

```yaml
tracing:
    level: info
    format: json
    sentry:
        dsn: "${SENTRY_DSN}"
        environment: "${APP_ENV:development}"
        sample_rate: 1.0
        traces_sample_rate: 0.1
```

When `dsn` is empty or the `sentry` section is omitted, Sentry is silently skipped. `environment` defaults to the value returned by `modo::config::env()` (the `APP_ENV` environment variable).

### Wiring the HTTP tracing middleware

The subscriber above captures all `tracing` events. To additionally get one
span per HTTP request, register `modo::middleware::tracing()` on the router:

```rust,no_run
use axum::Router;
use axum::routing::get;

async fn health() -> &'static str { "ok" }

let app: Router = Router::new()
    .route("/health", get(health))
    .layer(modo::middleware::tracing());
```

This installs a `tower_http::trace::TraceLayer` whose spans are produced by
`ModoMakeSpan`.

## Adding custom fields

HTTP request spans are created by `ModoMakeSpan` (in `modo::middleware`) and
pre-declare `tenant_id = tracing::field::Empty`. The tenant middleware later
calls `span.record("tenant_id", ...)` once the tenant is resolved, so the
final log line includes it.

`tracing` only accepts `record()` calls for fields that already exist on the
span, so **any field that later middleware needs to fill in must be
pre-declared on `ModoMakeSpan`** when the span is created.

### Extending `ModoMakeSpan`

To add a new middleware-recorded field (for example `request_id`), add it to
the `info_span!` invocation inside `ModoMakeSpan::make_span` with an empty
initial value:

```rust,ignore
// src/middleware/tracing.rs
tracing::info_span!(
    "http_request",
    method = %request.method(),
    uri = %request.uri(),
    version = ?request.version(),
    tenant_id = tracing::field::Empty,
    request_id = tracing::field::Empty, // new field
)
```

Middleware can then record it:

```rust,no_run
let span = tracing::Span::current();
span.record("request_id", "01H...");
```

### Ad-hoc fields from a handler

Handlers can attach per-event fields without modifying `ModoMakeSpan`:

```rust,no_run
use modo::tracing::info;

pub async fn create_order(user_id: String, order_id: String) -> modo::Result<()> {
    info!(user_id = %user_id, order_id = %order_id, "order created");
    Ok(())
}
```

## Logging conventions

Field names must be snake_case (`user_id`, `session_id`, `job_id`). The re-exported macros (`info!`, `debug!`, `warn!`, `error!`, `trace!`) are available as `modo::tracing::*`.
