# modo

[![docs.rs](https://img.shields.io/docsrs/modo)](https://docs.rs/modo)

Ergonomic Rust web framework for small monolithic apps. Single binary, compile-time route discovery, batteries included.

## Features

| Feature        | Enables                                                                                                                |
| -------------- | ---------------------------------------------------------------------------------------------------------------------- |
| `templates`    | MiniJinja template engine, `ViewRenderer`, `ViewResponse`, `#[view]`, `#[template_function]`, `#[template_filter]`    |
| `csrf`         | Double-submit cookie CSRF protection middleware and `CsrfToken` extractor                                              |
| `i18n`         | Translation store, `I18n` extractor, `#[t]` macro                                                                     |
| `sse`          | Server-Sent Events (`SseEvent`, `SseBroadcastManager`, `Sse` extractor)                                               |
| `static-fs`    | Filesystem static file serving (development)                                                                           |
| `sentry`       | Sentry error tracking (`SentryConfig`, `SentryConfigProvider`, request_id tagging)                                     |
| `static-embed` | Embedded static files via `rust-embed` (production)                                                                    |

## Usage

### Minimal application

```rust
use modo::{HandlerResult, HttpError};

#[modo::handler(GET, "/")]
async fn index(request_id: modo::RequestId) -> String {
    format!("Hello modo! (request: {request_id})")
}

#[modo::handler(GET, "/error")]
async fn error_example() -> Result<&'static str, HttpError> {
    Err(HttpError::NotFound)
}

#[modo::main]
async fn main(
    app: modo::app::AppBuilder,
    config: modo::config::AppConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    app.config(config).run().await
}
```

### Handlers with validation and sanitization

```rust
use modo::{HandlerResult, JsonResult};
use modo::extractor::{JsonReq, FormReq};

#[derive(serde::Deserialize, modo::Sanitize, modo::Validate)]
struct CreateUser {
    #[clean(trim, normalize_email)]
    #[validate(required, email)]
    email: String,

    #[clean(trim)]
    #[validate(required, min_length = 8)]
    password: String,
}

#[modo::handler(POST, "/users")]
async fn create_user(body: JsonReq<CreateUser>) -> JsonResult<&'static str> {
    body.validate()?;
    Ok(modo::Json("created"))
}
```

### Grouping routes into a module

```rust
#[modo::module("/api/v1", name = "api")]
mod api {}

#[modo::handler(GET, "/users", module = "api")]
async fn list_users() -> JsonResult<Vec<String>> {
    Ok(modo::Json(vec![]))
}
```

### Registering services

```rust,no_run
use modo::HandlerResult;
use modo_db::Db;

#[modo::handler(GET, "/data")]
async fn get_data(Db(db): Db) -> HandlerResult<String> {
    Ok("data".to_string())
}

#[modo::main]
async fn main(
    app: modo::app::AppBuilder,
    config: modo::config::AppConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let db = modo_db::connect(&Default::default()).await?;
    app.config(config).managed_service(db).run().await
}
```

### Cookies

```rust
use modo::cookies::CookieManager;

#[modo::handler(GET, "/set-cookie")]
async fn set_cookie(mut cookies: CookieManager) -> CookieManager {
    cookies.set("session", "abc123");
    cookies
}

#[modo::handler(GET, "/read-cookie")]
async fn read_cookie(cookies: CookieManager) -> String {
    cookies.get("session").unwrap_or_default()
}
```

### CORS

```rust
use modo::CorsConfig;

#[modo::main]
async fn main(
    app: modo::app::AppBuilder,
    config: modo::config::AppConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    app.config(config)
       .cors(CorsConfig::with_origins(&["https://example.com"]))
       .run()
       .await
}
```

### Graceful shutdown

```rust
use modo::GracefulShutdown;

struct JobQueue { /* ... */ }

impl GracefulShutdown for JobQueue {
    fn graceful_shutdown(
        &self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>> {
        Box::pin(async { /* drain jobs */ })
    }
}

#[modo::main]
async fn main(
    app: modo::app::AppBuilder,
    config: modo::config::AppConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    app.config(config).managed_service(JobQueue { /* ... */ }).run().await
}
```

## Configuration

Config is loaded from `config/{MODO_ENV}.yaml` (default: `config/development.yaml`).
`${VAR}` and `${VAR:-default}` patterns in the YAML are substituted from environment variables.

```yaml
server:
    port: 3000
    host: "0.0.0.0"
    secret_key: "${SECRET_KEY}"
    log_level: "info"
    shutdown_timeout_secs: 30
    hook_timeout_secs: 5
    http:
        timeout: 30 # request timeout in seconds; omit to disable
        body_limit: "2mb"
        compression: false
        catch_panic: true
        trailing_slash: none # none | strip | add
        maintenance: false
        sensitive_headers: true
    security_headers:
        enabled: true
        hsts: true
        hsts_max_age: 31536000
    rate_limit:
        requests: 100
        window_secs: 60
cookies:
    path: "/"
    secure: true
    http_only: true
    same_site: lax # strict | lax | none
```

## Key Types

| Type               | Purpose                                                           |
| ------------------ | ----------------------------------------------------------------- |
| `AppBuilder`       | Fluent builder — register services, layers, and call `.run()`     |
| `AppState`         | Shared axum state — services, server config, cookie key           |
| `ServiceRegistry`  | Type-map for application services                                 |
| `AppConfig`        | Top-level YAML config (deserialized by `#[modo::main]`)           |
| `HttpError`        | Ergonomic HTTP 4xx/5xx error enum with `IntoResponse`             |
| `Error`            | Structured error with status, code, message, and details          |
| `HandlerResult<T>` | `Result<T, Error>` alias for generic handlers                     |
| `JsonResult<T>`    | `Result<Json<T>, Error>` alias for JSON API handlers              |
| `CookieManager`    | Plain, signed, and encrypted cookie read/write extractor          |
| `JsonReq<T>`       | JSON request extractor with auto-sanitization                     |
| `FormReq<T>`       | Form request extractor with auto-sanitization                     |
| `PathReq<T>`       | Path parameter extractor (re-export of `axum::extract::Path`)     |
| `QueryReq<T>`      | Query string extractor (re-export of `axum::extract::Query`)      |
| `RequestId`        | ULID request ID injected by middleware and propagated via headers |
| `ClientIp`         | Resolved client IP (supports trusted proxies and Cloudflare)      |
| `RateLimitInfo`    | Rate limit header info available as a request extractor           |
| `GracefulShutdown` | Trait for services that participate in shutdown sequencing        |
| `ShutdownPhase`    | `Drain` (before user hooks) or `Close` (after user hooks)         |
| `CorsConfig`       | CORS policy (mirror, list, custom, any)                           |

### Feature-gated types

| Type                  | Feature     | Purpose                                                    |
| --------------------- | ----------- | ---------------------------------------------------------- |
| `ViewRenderer`        | `templates` | Explicit template rendering with HTMX detection            |
| `ViewResponse`        | `templates` | Handler return type for rendered HTML or redirects         |
| `ViewResult`          | `templates` | `Result<ViewResponse, Error>` alias                        |
| `CsrfToken`           | `csrf`      | CSRF token injected by middleware into request extensions  |
| `I18n`                | `i18n`      | Translation extractor with per-request language resolution |
| `SseEvent`            | `sse`       | Builder for a single SSE event                             |
| `SseBroadcastManager` | `sse`       | Keyed fan-out broadcast channels                           |
| `SseResponse`         | `sse`       | Handler return type wrapping an SSE stream                 |
| `Sse`                 | `sse`       | Extractor that applies keep-alive config to SSE responses  |
| `SentryConfig`        | `sentry`    | Sentry DSN, environment, and traces_sample_rate config     |
| `SentryConfigProvider` | `sentry`   | Trait for extracting Sentry config from custom configs     |
