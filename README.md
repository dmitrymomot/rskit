# modo

> **modo** (Latin: "way, method") — a Rust web framework for small monolithic apps.

[![CI](https://github.com/dmitrymomot/modo/actions/workflows/ci.yml/badge.svg)](https://github.com/dmitrymomot/modo/actions/workflows/ci.yml)
[![docs.rs](https://img.shields.io/docsrs/modo-rs)](https://docs.rs/modo-rs)
[![License: Apache-2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](LICENSE)
![Rust](https://img.shields.io/badge/rust-1.92+-orange.svg)

One crate. Zero proc macros. Everything you need to ship a real app — sessions, auth, background jobs, email, storage — without stitching together 15 crates and writing the glue yourself.

Built on [axum 0.8](https://github.com/tokio-rs/axum), so you keep full access to the axum/tower ecosystem. Handlers are plain `async fn`. Routes use axum's `Router`. Database queries use libsql directly. No magic, no code generation, no framework lock-in.

## Why modo

**You need 15+ crates for a real Rust web app.** Sessions, auth, background jobs, config, email, flash messages, rate limiting, CORS, CSRF — each one is a separate crate with its own patterns, its own wiring, and its own test setup. modo gives you all of it in one import.

**Proc macros slow you down.** They increase compile times, hide control flow, and make errors cryptic. modo uses zero proc macros. Handlers are plain functions. Routes are axum routes. What you see is what runs.

**Wiring everything together is the real work.** Config loading, service injection, middleware ordering, graceful shutdown — the framework should handle this, not you. With modo, it's one `Registry`, one `run!` macro, and you're done.

## Quick look

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

## What's included

### Config that just works

YAML files with `${ENV_VAR}` and `${ENV_VAR:default}` substitution, loaded per `APP_ENV`. No builder, no manual env parsing, no `.env` ceremony.

```yaml
# config/production.yaml
server:
  port: ${PORT:8080}
database:
  url: ${DATABASE_URL}
```

```rust
let config: Config = modo::config::load("config/")?;
```

### Database without an ORM

SQLite via libsql. A single `Database` handle wraps an `Arc`-ed connection — clone-friendly, no pool complexity.

```rust
let pool = db::connect(&config.database).await?;
db::migrate("migrations/", &pool).await?;
```

### Sessions with zero glue code

Database-backed, signed cookies, sliding expiry, multi-device, fingerprinting. The middleware handles the full request/response lifecycle — you just call methods.

```rust
async fn login(session: Session, JsonRequest(form): JsonRequest<LoginForm>) -> Result<()> {
    // ... validate credentials ...
    session.authenticate(user_id).await
}

async fn dashboard(session: Session) -> Result<String> {
    let uid = session.user_id().ok_or(Error::unauthorized("not logged in"))?;
    Ok(format!("Welcome, {uid}"))
}
```

### Auth without a framework

Password hashing (Argon2id), TOTP (Google Authenticator compatible), one-time codes, backup codes, JWT with middleware, OAuth2 (GitHub, Google) — all plain functions and types, no annotations.

```rust
let hash = auth::password::hash(password, &PasswordConfig::default()).await?;
let valid = auth::password::verify(password, &hash).await?;

let totp = Totp::from_base32(secret, &TotpConfig::default())?;
let ok = totp.verify(user_code);
```

### Background jobs as plain functions

SQLite-backed queue with retries, exponential backoff, timeouts, scheduled execution, and idempotent enqueue. Handlers use the same extraction pattern as HTTP routes.

```rust
async fn send_email(Payload(p): Payload<Email>, Service(mailer): Service<Mailer>) -> Result<()> {
    mailer.send(&p.to, &p.body).await
}

let worker = Worker::builder(&config.job, &registry)
    .register("send_email", send_email)
    .start().await;

Enqueuer::new(&pool).enqueue("send_email", &payload).await?;
```

### Graceful shutdown in one line

The `run!` macro waits for SIGTERM/SIGINT, then shuts down each component in declaration order. No cancellation tokens, no orchestration code.

```rust
modo::run!(worker, server).await
```

### Dependency injection without macros

`Registry` is a typed map. `.add()` at startup, `Service<T>` in handlers. No `#[inject]`, no container config, no runtime reflection.

```rust
let mut registry = Registry::new();
registry.add(pool);
registry.add(mailer);

// In any handler:
async fn list_users(Service(pool): Service<Pool>) -> Result<Json<Vec<User>>> { ... }
```

### Request extraction with auto-sanitization

`JsonRequest<T>`, `FormRequest<T>`, and `Query<T>` call your `Sanitize` impl before the handler runs. Define it once, applied everywhere.

### Middleware you'd write anyway

Rate limiting, CORS, CSRF, compression, security headers, request tracing, panic catching, error handler — all included with sensible defaults. All standard Tower layers, not a custom system.

### And the rest

| Module        | What it does                                                     |
| ------------- | ---------------------------------------------------------------- |
| `template`    | MiniJinja with i18n, HTMX detection, flash message integration   |
| `sse`         | Server-Sent Events with named broadcast channels                 |
| `email`       | Markdown-to-HTML email rendering with SMTP                       |
| `storage`     | S3-compatible object storage with ACL and upload-from-URL        |
| `webhook`     | Outbound webhook delivery with Standard Webhooks signing         |
| `dns`         | TXT/CNAME verification for custom domain validation              |
| `geolocation` | MaxMind GeoIP2 location lookup with middleware                   |
| `rbac`        | Role-based access control with guard middleware                  |
| `tenant`      | Multi-tenancy via subdomain, header, path, or custom resolver    |
| `flash`       | Signed, read-once cookie flash messages                          |
| `cron`        | Cron scheduling (5/6-field expressions)                          |
| `health`      | `/_live` and `/_ready` health check endpoints                    |
| `cache`       | In-memory LRU cache                                              |
| `testing`     | `TestDb`, `TestApp`, `TestSession` — in-process, no server needed|

## Feature flags

Core modules are always available: cache, config, cookie, cron, encoding, error, extractor, flash, health, id, ip, middleware, rbac, runtime, sanitize, server, service, tenant, tracing, validate.

Optional modules are behind feature flags:

```toml
[dependencies]
modo-rs = { version = "0.5", features = ["auth", "templates"] }
```

| Feature          | Modules                                               | Implies        |
| ---------------- | ----------------------------------------------------- | -------------- |
| `db` (default)   | Database handle, migrations, query traits, pagination  |                |
| `session`        | Database-backed sessions with signed cookies           | `db`           |
| `job`            | Background job queue with retries and scheduling       | `db`           |
| `http-client`    | HTTP client with retries, timeouts, connection pooling |                |
| `auth`           | Password hashing, JWT, OAuth2, TOTP, backup codes      | `http-client`  |
| `templates`      | MiniJinja engine with i18n and static file serving     |                |
| `sse`            | Server-Sent Events broadcasting                        |                |
| `email`          | Markdown-to-HTML email rendering with SMTP             |                |
| `storage`        | S3-compatible object storage with ACL                  | `http-client`  |
| `webhooks`       | Outbound webhook delivery with Standard Webhooks signing | `http-client` |
| `dns`            | DNS TXT/CNAME domain verification                      |                |
| `geolocation`    | MaxMind GeoIP2 location lookup                         |                |
| `qrcode`         | QR code generation with SVG rendering                  |                |
| `apikey`         | API key issuance, verification, and scoping            | `db`           |
| `text-embedding` | Text-to-vector embeddings (OpenAI, Gemini, Mistral, Voyage) | `http-client` |
| `tier`           | Feature-tier access control (plan-based gating)        |                |
| `sentry`         | Sentry error tracking and performance monitoring       |                |
| `test-helpers`   | TestDb, TestApp, TestSession, in-memory/stub backends  | `db`, `session`|
| `full`           | All of the above                                       |                |

## Re-exports

modo re-exports `axum`, `serde`, `serde_json`, and `tokio` so you don't need to version-match them yourself.

## Claude Code Plugin

The `modo-dev` plugin gives Claude Code full knowledge of modo's APIs and conventions.

```
/plugin marketplace add dmitrymomot/modo
/plugin install modo@modo-dev
/reload-plugins
```

Once installed, it activates automatically when you build with modo. Or invoke it with `/modo-dev`.

## Development

```sh
cargo check                                        # type check
cargo test                                         # run tests
cargo test --all-features                          # run all tests
cargo clippy --all-features --tests -- -D warnings # lint
cargo fmt --check                                  # format check
```

## License

Apache-2.0
