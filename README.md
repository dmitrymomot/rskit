# modo

> **modo** (Latin: "way, method") — _the way_ to build web apps with Rust.

[![CI](https://github.com/dmitrymomot/modo/actions/workflows/ci.yml/badge.svg)](https://github.com/dmitrymomot/modo/actions/workflows/ci.yml)
[![License: Apache-2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](LICENSE)
![Rust](https://img.shields.io/badge/rust-stable-orange.svg)

A batteries-included Rust framework for small monolithic web apps and APIs. Proc macros eliminate boilerplate, `inventory` auto-discovers routes at compile time, and the whole thing ships as a single binary. Built on [axum 0.8](https://github.com/tokio-rs/axum) with full access to the axum/tower ecosystem.

## Features

### Routing & Middleware

`#[handler]`, `#[module]`, and `#[main]` macros wire up your app — routes auto-register via `inventory`, middleware stacks at global, module, and handler levels.

### Database

SQLite and Postgres via feature flags. `#[entity]` macro generates SeaORM v2 models with auto-migration, timestamps, and built-in pagination.

### Sessions

Database-backed sessions with cookie fingerprinting, sliding expiry, multi-device tracking, and LRU cleanup.

### Authentication

`UserProvider` trait for pluggable auth. `Auth<U>` and `OptionalAuth<U>` extractors with Argon2id password hashing.

### Background Jobs

Persistent job queue with retries and exponential backoff. Cron scheduling, graceful shutdown, and `inventory`-based auto-discovery.

### File Uploads

`#[derive(FromMultipart)]` for declarative multipart parsing. Per-field validation (size, MIME type). Local and S3 storage backends via OpenDAL.

### Email

Markdown-to-HTML templates with SMTP and Resend transports. Multi-tenant sender profiles and locale-aware rendering.

### Frontend

MiniJinja templates, HTMX support, Server-Sent Events, CSRF protection, i18n, flash messages, and static file embedding.

## Quick Start

Install the CLI:

```sh
cargo install modo-cli
```

Scaffold a new project:

```sh
modo new my-app                       # web app (default)
modo new my-app --postgres --s3       # web app with PostgreSQL + S3
modo new my-api --template api        # JSON API
modo new my-worker --template worker  # background worker
modo new my-app --template minimal    # bare-bones, no database
```

| Template  | Description                                            | Database |
| --------- | ------------------------------------------------------ | -------- |
| `web`     | Full-stack with HTMX, auth, jobs, email, uploads, i18n | Optional |
| `api`     | JSON API with handlers and models                      | Optional |
| `worker`  | Background job worker, no HTTP handlers                | Optional |
| `minimal` | Bare-bones, config only                                | None     |

Database defaults to SQLite. Pass `--postgres` for PostgreSQL. Pass `--s3` to use RustFS (S3-compatible) for file uploads in development (web template only).

Then:

```sh
cd my-app
just dev
```

Here's what a handler looks like:

```rust
use modo::HandlerResult;
use modo::HttpError;
use modo::extractors::FormReq;

#[derive(serde::Deserialize, modo::Sanitize, modo::Validate)]
struct ContactForm {
    #[clean(trim, normalize_email)]
    #[validate(required, email)]
    email: String,

    #[clean(trim, strip_html_tags)]
    #[validate(required, min_length = 5, max_length = 1000)]
    message: String,
}

#[modo::handler(GET, "/")]
async fn index(request_id: RequestId) -> String {
    format!("Hello modo! (request: {request_id})")
}

#[modo::handler(GET, "/health")]
async fn health() -> &'static str {
    "ok"
}

#[modo::handler(GET, "/error")]
async fn error_example() -> Result<&'static str, HttpError> {
    Err(HttpError::NotFound)
}

#[modo::handler(POST, "/contact")]
async fn contact(form: FormReq<ContactForm>) -> HandlerResult<&'static str> {
    form.validate()?;
    Ok("Thanks for your message!")
}

#[modo::main]
async fn main(
    app: modo::app::AppBuilder,
    config: modo::config::AppConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    app.config(config).run().await
}
```

## Code Showcases

<details>
<summary><strong>Define a database entity</strong></summary>

```rust
#[modo_db::entity(table = "todos")]
#[entity(timestamps)]
pub struct Todo {
    #[entity(primary_key, auto = "ulid")]
    pub id: String,
    pub title: String,
    #[entity(default_value = false)]
    pub completed: bool,
}
```

The `#[entity]` macro generates a SeaORM v2 model, auto-migration, and column helpers. `timestamps` adds `created_at` / `updated_at` columns automatically.

</details>

<details>
<summary><strong>Background jobs</strong></summary>

Define jobs with the `#[job]` macro:

```rust
#[modo_jobs::job(queue = "default")]
async fn say_hello(payload: GreetingPayload) -> HandlerResult<()> {
    tracing::info!(name = %payload.name, "Hello, {}!", payload.name);
    Ok(())
}

#[modo_jobs::job(cron = "0 */1 * * * *", timeout = "30s")]
async fn heartbeat() -> HandlerResult<()> {
    tracing::info!("heartbeat tick");
    Ok(())
}
```

Enqueue from a handler:

```rust
#[modo::handler(POST, "/jobs/greet")]
async fn enqueue_greet(queue: JobQueue, input: Json<GreetingPayload>) -> JsonResult<Value> {
    let job_id = SayHelloJob::enqueue(&queue, &input).await?;
    Ok(Json(json!({ "job_id": job_id.to_string() })))
}
```

</details>

<details>
<summary><strong>File uploads</strong></summary>

Declare a multipart form with per-field validation:

```rust
#[derive(FromMultipart, modo::Sanitize, modo::Validate)]
pub struct ProfileForm {
    #[upload(max_size = "5mb", accept = "image/*")]
    pub avatar: UploadedFile,

    #[clean(trim)]
    #[validate(required, min_length = 2)]
    pub name: String,

    #[clean(trim, normalize_email)]
    #[validate(required, email)]
    pub email: String,
}
```

Handle the upload:

```rust
#[modo::handler(POST, "/profile")]
async fn update_profile(
    storage: Service<Box<dyn FileStorage>>,
    form: MultipartForm<ProfileForm>,
) -> JsonResult<serde_json::Value> {
    form.validate()?;
    let stored = storage.store("avatars", &form.avatar).await?;
    Ok(modo::Json(serde_json::json!({
        "name": form.name,
        "avatar_path": stored.path,
    })))
}
```

</details>

## Workspace Crates

| Crate                | Description                                                                  |
| -------------------- | ---------------------------------------------------------------------------- |
| `modo`               | Core — routing, middleware, cookies, config, services                        |
| `modo-macros`        | Proc macros (`#[handler]`, `#[main]`, `#[module]`)                           |
| `modo-db`            | Database layer — SQLite and Postgres via SeaORM v2                           |
| `modo-db-macros`     | `#[entity]` proc macro for model and migration generation                    |
| `modo-email`         | Email with Markdown templates, SMTP/Resend, multi-tenant sender profiles     |
| `modo-jobs`          | Background job queue with retries, cron, and graceful shutdown               |
| `modo-jobs-macros`   | `#[job]` proc macro for declarative job definition                           |
| `modo-session`       | Database-backed sessions with fingerprinting and multi-device support        |
| `modo-auth`          | Authentication — `UserProvider` trait, `Auth<U>` extractor, Argon2id         |
| `modo-tenant`        | Multi-tenancy — subdomain/header/path resolution, template context injection |
| `modo-upload`        | File uploads — local and S3 storage via OpenDAL                              |
| `modo-upload-macros` | `#[derive(FromMultipart)]` proc macro                                        |
| `modo-cli`           | CLI tool for scaffolding modo projects                                       |

## Examples

| Example         | Description                                             | Run                          |
| --------------- | ------------------------------------------------------- | ---------------------------- |
| `hello`         | Minimal app with handlers, validation, and sanitization | `cargo run -p hello`         |
| `todo-api`      | RESTful JSON API with SQLite persistence                | `cargo run -p todo-api`      |
| `jobs`          | Background job queue with cron scheduling               | `cargo run -p jobs`          |
| `upload`        | File uploads with multipart parsing and storage         | `cargo run -p upload`        |
| `templates`     | MiniJinja template rendering                            | `cargo run -p templates`     |
| `sse-chat`      | Real-time chat with SSE, sessions, and CSRF             | `cargo run -p sse-chat`      |
| `sse-dashboard` | Live-updating dashboard with SSE streaming              | `cargo run -p sse-dashboard` |

## Feature Flags

The `modo` core crate has these optional features (all off by default):

| Flag           | Description                                        |
| -------------- | -------------------------------------------------- |
| `templates`    | MiniJinja template engine integration              |
| `csrf`         | Double-submit signed-cookie CSRF protection        |
| `i18n`         | Locale detection and internationalization          |
| `sse`          | Server-Sent Events support                         |
| `static-fs`    | Serve static files from a directory at runtime     |
| `static-embed` | Embed static files into the binary at compile time |

## Project Status

**Beta** — under active development. APIs may still change.

Found a bug or have a feature request? [Open an issue](https://github.com/dmitrymomot/modo/issues).

## License

[Apache-2.0](LICENSE)
