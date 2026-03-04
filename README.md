# rskit

Rust web framework for micro-SaaS. Single binary, SQLite-only, maximum compile-time magic.

[![CI](https://github.com/dmitrymomot/rskit/actions/workflows/ci.yml/badge.svg)](https://github.com/dmitrymomot/rskit/actions/workflows/ci.yml)
[![License: Apache-2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](LICENSE)
![Rust](https://img.shields.io/badge/rust-stable-orange.svg)

## Features

- **Proc-macro driven** — `#[rskit::handler]` and `#[rskit::main]` for zero-boilerplate routing
- **Auto-discovery** — handlers register themselves via `inventory`, no manual router wiring
- **Module system** — group routes with shared prefixes and middleware via `#[rskit::module]`
- **SQLite-only** — WAL mode, single file, no external DB servers
- **SeaORM v2** — async database access with the `Db` extractor
- **CSRF protection** — double-submit signed cookie, built-in middleware
- **Flash messages** — cookie-based, one-shot, no session dependency
- **HTMX support** — Askama templates with `BaseContext` extractor for HTMX, flash, CSRF, and locale
- **Built on axum 0.8** — full access to the axum/tower ecosystem

## Quick Start

```rust
use rskit::error::RskitError;

#[rskit::handler(GET, "/")]
async fn index() -> &'static str {
    "Hello rskit!"
}

#[rskit::handler(GET, "/health")]
async fn health() -> &'static str {
    "ok"
}

#[rskit::handler(GET, "/error")]
async fn error_example() -> Result<&'static str, RskitError> {
    Err(RskitError::NotFound)
}

#[rskit::main]
async fn main(app: rskit::app::AppBuilder) -> Result<(), Box<dyn std::error::Error>> {
    app.run().await
}
```

## Project Status

**Alpha** — under active development. APIs will change. Not production-ready.

## License

[Apache-2.0](LICENSE)
