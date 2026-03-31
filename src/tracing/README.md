# modo::tracing

Tracing initialisation and structured logging for modo applications.

Wraps [`tracing_subscriber`](https://docs.rs/tracing-subscriber) with a simple YAML-driven configuration and an optional Sentry integration. Call `init` once at startup and hold the returned `TracingGuard` for the process lifetime.

This module handles **subscriber setup** (log format, level, Sentry). For HTTP request/response tracing, see `modo::middleware::tracing()` which creates a `TraceLayer` with `ModoMakeSpan`.

## Features

| Feature  | What it adds                                                                            |
| -------- | --------------------------------------------------------------------------------------- |
| `sentry` | Initialises the Sentry SDK and wires it to the tracing subscriber via `sentry-tracing`. |

## Key types

| Type / Item    | Description                                                                                         |
| -------------- | --------------------------------------------------------------------------------------------------- |
| `Config`       | Log level and output format; optionally embeds `SentryConfig` when the `sentry` feature is enabled. |
| `init`         | Initialises the global tracing subscriber and optional Sentry client; returns `TracingGuard`.        |
| `TracingGuard` | RAII guard that keeps the subscriber and Sentry client alive. Implements `Task` and `Default`.       |
| `SentryConfig` | Sentry DSN, environment tag, and sampling rates. Only present with the `sentry` feature.            |
| `info!` etc.   | Re-exports of `tracing::{debug, error, info, trace, warn}` for convenience.                         |

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

### With Sentry (requires `sentry` feature)

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

When `dsn` is empty or absent, Sentry is silently skipped.

## Logging conventions

Field names must be snake_case (`user_id`, `session_id`, `job_id`). The re-exported macros (`info!`, `debug!`, `warn!`, `error!`, `trace!`) are available as `modo::tracing::*`.
