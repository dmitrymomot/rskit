# modo

> **modo** (Latin: "way, method") — _the way_ to build micro-SaaS with Rust.
> Single binary, compile-time magic, multi-DB support.

[![CI](https://github.com/dmitrymomot/modo/actions/workflows/ci.yml/badge.svg)](https://github.com/dmitrymomot/modo/actions/workflows/ci.yml)
[![License: Apache-2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](LICENSE)
![Rust](https://img.shields.io/badge/rust-stable-orange.svg)

## Features

- **Proc-macro driven** — `#[modo::handler]` and `#[modo::main]` for zero-boilerplate routing
- **Auto-discovery** — handlers register themselves via `inventory`, no manual router wiring
- **Module system** — group routes with shared prefixes and middleware via `#[modo::module]`
- **Multi-DB** — SQLite (WAL mode) and Postgres via feature flags
- **SeaORM v2** — async database access with the `Db` extractor
- **Sessions** — cookie-based sessions with fingerprinting, touch, and multi-device management
- **Authentication** — `UserProvider` trait with `Auth<U>` / `OptionalAuth<U>` extractors
- **Background jobs** — persistent job queue with retries, cron scheduling, and graceful shutdown
- **File uploads** — streaming uploads with configurable storage backends
- **CSRF protection** — double-submit signed cookie, built-in middleware
- **Flash messages** — cookie-based, one-shot, no session dependency
- **HTMX support** — Askama templates with `BaseContext` extractor for HTMX, flash, CSRF, and locale
- **Built on axum 0.8** — full access to the axum/tower ecosystem

## Workspace Crates

| Crate                | Description                                             |
| -------------------- | ------------------------------------------------------- |
| `modo`               | Core — HTTP, cookies, services                          |
| `modo-macros`        | Core proc macros (`#[handler]`, `#[main]`, `#[module]`) |
| `modo-db`            | Database layer (features: `sqlite`, `postgres`)         |
| `modo-db-macros`     | Database proc macros (`#[entity]`)                      |
| `modo-session`       | Session management                                      |
| `modo-auth`          | Authentication extractors                               |
| `modo-jobs`          | Background job queue                                    |
| `modo-jobs-macros`   | `#[job(...)]` proc macro                                |
| `modo-upload`        | File uploads                                            |
| `modo-upload-macros` | Upload proc macros                                      |

## Quick Start

```rust
use modo::error::Error;

#[modo::handler(GET, "/")]
async fn index() -> &'static str {
    "Hello modo!"
}

#[modo::handler(GET, "/health")]
async fn health() -> &'static str {
    "ok"
}

#[modo::handler(GET, "/error")]
async fn error_example() -> Result<&'static str, Error> {
    Err(Error::NotFound)
}

#[modo::main]
async fn main(app: modo::app::AppBuilder) -> Result<(), Box<dyn std::error::Error>> {
    app.run().await
}
```

## Project Status

**Alpha** — under active development. APIs will change. Not production-ready.

## License

[Apache-2.0](LICENSE)
