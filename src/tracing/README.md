# modo::tracing

Tracing initialisation and structured logging for modo applications.

Wraps [`tracing_subscriber`](https://docs.rs/tracing-subscriber) with a simple YAML-driven configuration and a built-in Sentry integration. Call `init` once at startup and hold the returned `TracingGuard` for the process lifetime.

This module handles **subscriber setup** (log format, level, Sentry). For HTTP request/response tracing, see `modo::middleware::tracing()` which creates a `TraceLayer` with `ModoMakeSpan`.

## Key types

| Type / Item    | Description                                                                                   |
| -------------- | --------------------------------------------------------------------------------------------- |
| `Config`       | Log level, output format, and optional `SentryConfig`.                                        |
| `init`         | Initialises the global tracing subscriber and optional Sentry client; returns `TracingGuard`. |
| `TracingGuard` | RAII guard that keeps the subscriber and Sentry client alive. Implements `Task` and `Default`. |
| `SentryConfig` | Sentry DSN, environment tag, and sampling rates.                                              |
| `info!` etc.   | Re-exports of `tracing::{debug, error, info, trace, warn}` for convenience.                   |

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

## Request span fields

HTTP request spans are created by `modo::middleware::tracing()` using a `ModoMakeSpan` that pre-declares `tenant_id = tracing::field::Empty`. The tenant middleware then calls `span.record("tenant_id", ...)` once the tenant is resolved, so the final log line includes the tenant identifier.

Any additional field that later middleware needs to fill in must be pre-declared on `ModoMakeSpan` — tracing only accepts `record()` calls for fields that already exist on the span.

### Recording custom fields from a handler

Handlers can attach ad-hoc fields to the active span without modifying `ModoMakeSpan`:

```rust,no_run
use modo::tracing::info;

pub async fn create_order(user_id: String, order_id: String) -> modo::Result<()> {
    info!(user_id = %user_id, order_id = %order_id, "order created");
    Ok(())
}
```

## Logging conventions

Field names must be snake_case (`user_id`, `session_id`, `job_id`). The re-exported macros (`info!`, `debug!`, `warn!`, `error!`, `trace!`) are available as `modo::tracing::*`.
