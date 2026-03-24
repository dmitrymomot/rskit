# modo

> **modo** (Latin: "way, method") — a Rust web framework for small monolithic apps.

[![CI](https://github.com/dmitrymomot/modo/actions/workflows/ci.yml/badge.svg)](https://github.com/dmitrymomot/modo/actions/workflows/ci.yml)
[![License: Apache-2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](LICENSE)
![Rust](https://img.shields.io/badge/rust-stable-orange.svg)

Single crate, zero proc macros. Handlers are plain `async fn`, routes use axum's `Router` directly, services are wired explicitly in `main()`, and database queries use raw sqlx. Built on [axum 0.8](https://github.com/tokio-rs/axum) with full access to the axum/tower ecosystem.

## Design

- **One crate** — `cargo add modo`, feature-flag what you need
- **Plain functions** — handlers are `async fn`, no macro magic
- **Explicit wiring** — routes, services, and middleware composed in `main()`
- **Raw sqlx** — no ORM, no generated models, just SQL
- **Feature flags** — optional modules enabled only when needed

## Modules

### Always available

| Module       | Description                                                                              |
| ------------ | ---------------------------------------------------------------------------------------- |
| `config`     | YAML config with `${VAR}` / `${VAR:default}` env var substitution, loaded per `APP_ENV`  |
| `db`         | SQLite via sqlx — `Pool`, `ReadPool`, `WritePool` newtypes with `Reader`/`Writer` traits |
| `server`     | Configurable HTTP server with graceful shutdown                                          |
| `error`      | `Error` type with status + message + optional source; `Result<T>` alias                  |
| `extractor`  | `Service<T>` (registry), `JsonRequest<T>` / `FormRequest<T>` (request bodies)            |
| `session`    | Cookie-based sessions with database backend, sliding expiry, multi-device                |
| `tenant`     | Multi-tenancy via subdomain, header, path, or custom resolver                            |
| `rbac`       | Role-based access control with `RoleExtractor` trait and guard middleware                |
| `job`        | Persistent background job queue with retries and exponential backoff                     |
| `cron`       | Cron scheduling with `croner` (5/6-field expressions)                                    |
| `flash`      | Cookie-based flash messages (signed, read-once-and-clear)                                |
| `cache`      | In-memory LRU cache                                                                      |
| `encoding`   | Base32 and base64url encode/decode                                                       |
| `id`         | ULID and short time-sortable ID generation                                               |
| `ip`         | Client IP extraction with trusted proxy support                                          |
| `middleware` | Rate limiting, request tracing, CORS, compression                                        |
| `runtime`    | `Task` trait + `run!` macro for sequential graceful shutdown                             |
| `sanitize`   | `Sanitize` trait for input sanitization                                                  |
| `validate`   | `Validate` trait with `ValidationError`                                                  |
| `cookie`     | Signed/private cookie jar utilities                                                      |
| `tracing`    | Structured logging with `tracing`                                                        |

### Feature-gated

| Feature        | Module        | Description                                                                             |
| -------------- | ------------- | --------------------------------------------------------------------------------------- |
| `auth`         | `auth::oauth` | OAuth2 (GitHub, Google) with pluggable `OAuthProvider` trait                            |
| `auth`         | `auth::jwt`   | JWT encode/decode with HMAC signing, middleware, bearer extraction, optional revocation |
| `templates`    | `template`    | MiniJinja templates with i18n, HTMX support, flash message integration                  |
| `sse`          | `sse`         | Server-Sent Events broadcasting with named channels                                     |
| `email`        | `email`       | Markdown-to-HTML email rendering with SMTP transport                                    |
| `storage`      | `storage`     | S3-compatible object storage with ACL support and upload-from-URL                       |
| `webhooks`     | `webhook`     | Outbound webhook delivery with Standard Webhooks signing                                |
| `dns`          | `dns`         | DNS TXT/CNAME verification for custom domain validation                                 |
| `geolocation`  | `geolocation` | MaxMind GeoIP2 location lookup with middleware                                          |
| `sentry`       | —             | Sentry error tracking integration                                                       |
| `test-helpers` | `testing`     | Test utilities (`TestDb`, `TestApp`, etc.)                                              |

## Quick start

```toml
[dependencies]
modo = "0.1"
```

Enable features as needed:

```toml
[dependencies]
modo = { version = "0.1", features = ["templates", "auth"] }
```

Or enable everything:

```toml
[dependencies]
modo = { version = "0.1", features = ["full"] }
```

### Minimal example

```rust
use modo::{Config, Result};
use modo::axum::{Router, routing::get};
use modo::runtime::Task;

async fn hello() -> &'static str {
    "Hello, modo!"
}

#[tokio::main]
async fn main() -> Result<()> {
    let config: Config = modo::config::load("config/")?;

    let app = Router::new()
        .route("/", get(hello));

    let server = modo::server::http(app, &config.server).await?;
    modo::run!(server).await
}
```

## Development

```sh
cargo check              # type check
cargo test               # run tests (default features)
cargo test --all-features # run all tests
cargo clippy --all-features --tests -- -D warnings  # lint
cargo fmt --check        # format check
```

## Claude Code Plugin

The `modo-dev` plugin gives Claude Code knowledge of modo's APIs, conventions, and patterns so it can help you build applications with the framework.

### Install

Inside an active Claude Code session, run:

```
/plugin marketplace add dmitrymomot/modo
/plugin install modo-dev@dmitrymomot-modo
/reload-plugins
```

### Usage

Once installed, the `modo-dev` skill activates automatically when you ask Claude Code to build something with modo. You can also invoke it explicitly:

```
/modo-dev
```

The skill covers handlers, routing, middleware, database, sessions, auth, RBAC, templates, SSE, jobs, cron, email, storage, webhooks, DNS verification, geolocation, multi-tenancy, flash messages, configuration, and testing.

## License

Apache-2.0
