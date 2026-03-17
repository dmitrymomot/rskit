# modo Conventions Reference

## Documentation

- **modo (umbrella crate):** https://docs.rs/modo

The `modo` crate is the umbrella re-export that application code depends on. It re-exports types from internal sub-crates (`modo-macros`, `modo-db`, `modo-jobs`, etc.) so you rarely import sub-crates directly. When in doubt, check the modo docs.rs page for the canonical public API surface. All macro attributes (`#[handler]`, `#[module]`, `#[view]`, etc.) are also re-exported through modo, so the only time you import a sub-crate directly is when using `modo_db`, `modo_jobs`, or similar standalone crate macros.

---

## File Organization

`mod.rs` has exactly one purpose in modo: it is the module import and re-export hub. No handler logic, view logic, task logic, or business code belongs there.

Rules:
- `mod.rs` — only `mod` declarations and `pub use` re-exports
- `handlers.rs` (or `handlers/`) — all `#[handler]`-annotated functions
- `views.rs` (or `views/`) — all `#[view]`-annotated structs and template helpers
- Tasks, jobs, and other domain code each go in their own files

```
src/
  users/
    mod.rs        ← ONLY: mod handlers; mod views; pub use ...
    handlers.rs   ← all #[handler] fns for this module
    views.rs      ← all #[view] structs for this module
    tasks.rs      ← background task logic
```

This rule is enforced by convention across the entire codebase. Violating it causes confusion about where code lives and makes the `mod.rs` hard to scan.

---

## Error Handling

### Result Type Aliases

Three result aliases cover all handler scenarios. Each defaults its error type to `modo::Error` but accepts a custom error as a second type parameter.

```rust
// Generic handler — use when returning non-JSON, non-template responses
pub type HandlerResult<T, E = Error> = Result<T, E>;

// JSON API handler — wraps the Ok value in axum::Json automatically
pub type JsonResult<T, E = Error> = Result<axum::Json<T>, E>;

// Template handler — returns ViewResponse (requires "templates" feature)
pub type ViewResult<E = Error> = Result<crate::templates::ViewResponse, E>;
```

### When to Use Which

| Scenario | Return type |
|---|---|
| JSON REST API endpoint | `JsonResult<T>` |
| HTMX or server-rendered HTML | `ViewResult` |
| Response with custom status, redirect, stream | `HandlerResult<impl IntoResponse>` |
| Handler with a custom domain error type | `JsonResult<T, MyError>` |

### The `Error` Type

`modo::Error` is a structured HTTP error carrying status code, machine-readable code string, human-readable message, and an optional details map:

```rust
pub struct Error {
    status: StatusCode,
    code: String,
    message: String,
    details: HashMap<String, serde_json::Value>,
    source: Option<Arc<dyn std::error::Error + Send + Sync>>,
}
```

Construction methods:

```rust
// From HttpError variant (most common)
HttpError::NotFound.into()

// HttpError variant with custom message
HttpError::Forbidden.with_message("You do not own this resource")

// Convenience for 500
Error::internal("database connection failed")

// Full builder
Error::new(StatusCode::UNPROCESSABLE_ENTITY, "validation_failed", "Email is invalid")
    .detail("field", json!("email"))

// From anyhow — maps to 500
Error::from(anyhow_error)
```

### `HttpError` Variants

`HttpError` is a `Copy` enum covering all standard 4xx and 5xx codes. Every variant implements `IntoResponse` and converts to `Error` via `From`. Key variants:

```
BadRequest, Unauthorized, Forbidden, NotFound, Conflict,
UnprocessableEntity, TooManyRequests, InternalServerError,
ServiceUnavailable, GatewayTimeout
```

See `modo::HttpError` for the full list of variants.

### JSON Error Response Shape

The default JSON response for a 4xx error:

```json
{
  "error": "not_found",
  "message": "Not found",
  "status": 404,
  "details": { ... }
}
```

For 5xx errors, `default_response()` always returns the generic `InternalServerError` shape — the actual message is logged server-side but not exposed to the client.

### Custom Error Handlers via `#[error_handler]`

Register one global error handler using the `#[error_handler]` proc macro. It receives the `Error` and an `ErrorContext` (method, URI, headers) and returns a `Response`. The handler is collected via `inventory` and applied automatically through `error_handler_middleware`.

```rust
use modo::{Error, ErrorContext};
use axum::response::Response;

#[modo::error_handler]
fn my_error_handler(err: Error, ctx: &ErrorContext) -> Response {
    if ctx.accepts_html() {
        // Return an HTML error page
        render_error_page(err.status_code(), err.message_str()).into_response()
    } else {
        // Fall back to default JSON rendering
        err.default_response()
    }
}
```

`ErrorContext` helpers:
- `ctx.accepts_html()` — true when `Accept` contains `text/html`
- `ctx.is_htmx()` — true when `HX-Request` header is present

The `error_handler_middleware` is only wired into the stack when at least one `ErrorHandlerRegistration` is found via `inventory::iter`. If no `#[error_handler]` is defined, the default `Error::default_response()` is used directly.

---

## Middleware Stacking Order

Middleware in modo is applied in three scopes. When reading execution order, the scope hierarchy is:

```
Global (outermost) → Module → Handler (innermost)
```

A request enters Global middleware first, then Module middleware for its route, then Handler-level middleware, and finally reaches the handler. Responses traverse the stack in reverse.

### Framework-Level Stack

`AppBuilder::run()` assembles the full middleware stack. The comment in `app.rs` gives the exact order (outermost to innermost):

```
CORS
  Maintenance
    Catch Panic
      Request ID
        Sentry Hub              ← (sentry feature only)
          Sentry Request ID Tag ← (sentry feature only)
            Sentry HTTP         ← (sentry feature only)
              Sensitive Headers
                Tracing
                  Client IP
                    Timeout
                      Trailing Slash
                        Compression
                          Body Limit
                            Security Headers
                              Error Handler
                                Rate Limiter
                                  i18n
                                    Template Context
                                      Request ID Injector
                                        User Global Layers  ← AppBuilder::layer()
                                          Render Layer
                                            Module Middleware
                                              Handler Middleware (innermost)
```

See `AppBuilder::run()` in `modo/src/app.rs` for the exact assembly order.

### AppBuilder API

```rust
#[modo::main]
async fn main(app: modo::app::AppBuilder, config: AppConfig) {
    app
        // Register a service accessible via Service<T> extractor
        .service(my_db_pool)
        // Register a service that also participates in graceful shutdown
        .managed_service(my_job_queue)
        // Add a global Tower layer (applied outside module/handler middleware)
        .layer(my_auth_layer)
        // Configure CORS
        .cors(CorsConfig::permissive())
        // Override HTTP settings
        .timeout(30)
        .body_limit("10mb")
        .compression(true)
        // Register a shutdown hook (runs after HTTP draining)
        .on_shutdown(|| async { cleanup().await })
        // Add a readiness check exposed at /_ready
        .readiness_check(|| async { db_ping().await.map_err(Into::into) })
        .run()
        .await
        .unwrap();
}
```

### Module-Level Middleware

Declared on the `#[module]` attribute. Applied to all routes within the module's prefix:

```rust
#[modo::module(prefix = "/api/v1", middleware = [require_auth])]
mod api_v1 {
    #[modo::handler(GET, "/users")]
    async fn list_users() -> &'static str { "users" }
}
```

### Handler-Level Middleware

Declared via a separate `#[middleware(...)]` attribute on the handler function. Applied only to that specific route:

```rust
#[modo::handler(GET, "/users/{id}")]
#[middleware(rate_limit_strict)]
async fn get_user(id: String, /* ... */) -> JsonResult<UserResponse> { /* ... */ }
```

### Stacking Rules

- Multiple middleware on the same scope are applied **last-declared = innermost**. Note: this is the inverse of axum's raw `.layer()` where the last call wraps outermost. The macro achieves this by reversing the declaration order before applying layers.
- `AppBuilder::layer()` inserts global layers between the framework infrastructure stack and the template/module layers. Multiple calls to `.layer()` stack the same way: last call = outermost of the user layers.
- Template layers (`RenderLayer`, `TemplateContextLayer`) are auto-registered when `TemplateEngine` is present as a service — no manual `.layer()` call needed.

---

## API vs Web Error Handling

### JSON API Handlers

Return `JsonResult<T>` and use `HttpError` variants or `Error::internal()` for failures:

```rust
#[modo::handler(GET, "/users/{id}")]
async fn get_user(
    id: String,
    Service(db): Service<DatabaseService>,
) -> JsonResult<UserResponse> {
    let user = db.find_user(&id).await
        .map_err(|e| Error::internal(e.to_string()))?;
    let user = user.ok_or(HttpError::NotFound)?;
    Ok(Json(UserResponse::from(user)))
}
```

Key points:
- `JsonResult<T>` wraps the `Ok` value in `axum::Json` — return `Ok(Json(value))` not `Ok(value)`
- Use `modo::Json`, not `modo::axum::Json` (they are the same type, but the re-export path matters for imports)
- `?` on `Error` works because `Error` implements `IntoResponse`
- `HttpError::NotFound` converts to `Error` via `From<HttpError>` and then to a 404 JSON response

### Web / Template Handlers

Return `ViewResult<>` for server-rendered pages:

```rust
#[modo::handler(GET, "/dashboard")]
async fn dashboard(
    view: ViewRenderer,
    Service(db): Service<DatabaseService>,
) -> ViewResult {
    let data = db.fetch_summary().await
        .map_err(|e| Error::internal(e.to_string()))?;
    view.render(DashboardPage { summary: data })
}
```

Key points:
- HTMX requests (`HX-Request` header present) receive a partial template render, always HTTP 200
- Non-200 status codes skip HTMX rendering — errors returned from handlers do not attempt to render a template
- Use `#[error_handler]` with `ctx.is_htmx()` to return HTMX-compatible error fragments

### Content Negotiation in Error Handlers

A single `#[error_handler]` can serve both JSON and HTML clients:

```rust
#[modo::error_handler]
fn handle_error(err: Error, ctx: &ErrorContext) -> Response {
    if ctx.is_htmx() {
        // Return HTMX-compatible HTML fragment, always 200
        (StatusCode::OK, Html(format!("<div class='error'>{}</div>", err.message_str())))
            .into_response()
    } else if ctx.accepts_html() {
        // Full-page HTML error response
        render_error_page(err).into_response()
    } else {
        // Default structured JSON
        err.default_response()
    }
}
```

---

## Common Multi-Module Workflows

| Workflow | Reference files to read (in order) |
|---|---|
| Authenticated CRUD API | `conventions.md` → `database.md` → `handlers.md` → `auth-sessions.md` |
| Web form with validation | `conventions.md` → `handlers.md` → `templates-htmx.md` |
| Background email on user action | `handlers.md` → `jobs.md` → `email.md` |
| File upload with auth | `auth-sessions.md` → `upload.md` → `handlers.md` |
| Multi-tenant web app | `tenant.md` → `database.md` → `templates-htmx.md` |
| HTMX live dashboard | `templates-htmx.md` → `auth-sessions.md` |
| Full-stack feature (entity → API → job → email) | `conventions.md` → `database.md` → `handlers.md` → `jobs.md` → `email.md` |

---

## Gotchas

### inventory Linking in Tests

`inventory` registrations from library crates may not link when running unit tests. If `inventory::iter` returns nothing in a test that expects registrations, force the linker to include the registration with a wildcard import:

```rust
// In your test file or test module
use crate::entity::my_entity as _;
```

This is only needed in test binaries. The main binary links correctly.

### SeaORM ExprTrait Conflicts with Ord

SeaORM's `ExprTrait` adds `.max()` and `.min()` methods to expression types, which conflicts with `Ord::max` and `Ord::min`. When you see ambiguity errors, use the fully qualified syntax:

```rust
// Wrong — ambiguous when ExprTrait is in scope
let x = a.max(b);

// Correct
let x = Ord::max(a, b);
```

### HTMX 200-Only Rendering

The template render layer only renders HTMX responses when the HTTP status is 200. If a handler returns a non-200 status (e.g., a redirect or an error), the HTMX partial template will not be rendered. Design HTMX error flows through `#[error_handler]` returning 200 with an error fragment, not through non-200 handler returns.

### Alphabetical Re-exports in lib.rs

All `pub use` re-exports in `modo/src/lib.rs` must be sorted alphabetically. `cargo fmt` enforces this ordering. If you add a new re-export and `cargo fmt` reorders it, that is correct behavior.

Current public re-exports (from `lib.rs`):
- `axum::Json` — the canonical JSON responder (re-exported as `modo::Json`)
- `AppBuilder`, `AppState`, `ServiceRegistry`
- `AppConfig`, `HttpConfig`, `RateLimitConfig`, `SecurityHeadersConfig`, `ServerConfig`, `TrailingSlash`
- `CookieConfig`, `CookieManager`, `CookieOptions`, `SameSite`
- `CorsConfig`
- `CsrfConfig`, `CsrfToken` (behind `#[cfg(feature = "csrf")]`)
- `Error`, `ErrorContext`, `ErrorHandlerFn`, `ErrorHandlerRegistration`, `HandlerResult`, `HttpError`, `JsonResult`
- `ViewResult` (behind `#[cfg(feature = "templates")]`)
- `Service`
- `I18n`, `I18nConfig` (behind `#[cfg(feature = "i18n")]`)
- `ClientIp`, `OptionalRateLimitInfo`, `RateLimitInfo`
- `RequestId`
- `Method`
- `Sanitize`, `Validate`
- `SentryConfig`, `SentryConfigProvider` (behind `#[cfg(feature = "sentry")]`)
- `GracefulShutdown`, `ShutdownPhase`
- `TemplateConfig`, `TemplateContext`, `TemplateEngine`, `ViewRender`, `ViewRenderer`, `ViewResponse` (behind `#[cfg(feature = "templates")]`)

### Use `modo::Json` for Responses, `modo::extractor::JsonReq` for Requests

`modo::Json` re-exports `axum::Json` — use it for **response** wrapping (e.g. `Ok(Json(value))`).
`modo::extractor::JsonReq<T>` is the **request** extractor with auto-sanitization and validation.
`modo::extractor::FormReq<T>` is the form **request** extractor with auto-sanitization.
`modo::extractor::QueryReq<T>` re-exports `axum::extract::Query<T>` for query parameter extraction.

### ULID Session IDs — Never UUID

Session IDs are ULID strings throughout the framework. Never introduce UUID for session or entity identifiers. ULID is re-exported from modo: `use modo::ulid`.

### Feature Flag Syntax

Optional dependencies must use the `dep:name` syntax in `Cargo.toml`:

```toml
[dependencies]
some-crate = { version = "...", optional = true }

[features]
my-feature = ["dep:some-crate"]
```

In Rust source, gate code with `#[cfg(feature = "my-feature")]`. Proc macros cannot inspect `cfg` flags at expansion time — generated code must emit both branches explicitly:

```rust
// In proc macro output — emit both branches
#[cfg(feature = "templates")]
{ /* template path */ }
#[cfg(not(feature = "templates"))]
{ /* non-template path */ }
```

### `just test` Does Not Use `--all-features`

`just test` runs tests without `--all-features`. Feature-gated code requires targeted test invocations:

```bash
cargo test -p modo_auth --features session
```

`just lint` does use `--all-features`, so lint passes even when test doesn't cover feature-gated paths.

### Email Registration in Web Projects

When using `modo-email` in a web application, the mailer is registered as a jobs service — not as an app service:

```rust
// Correct: register on the jobs builder
let jobs = modo_jobs::new(&db, &config.jobs)
    .service(db.clone())
    .service(email_service)   // ← on jobs, NOT app
    .run()
    .await?;

// Register jobs as a managed service for graceful shutdown
app.managed_service(jobs)
```

Do not call `.service(email)` on the `AppBuilder`. The app enqueues `SendEmailPayload`; the job worker handles delivery.

### Cron Jobs Are In-Memory Only

Cron jobs defined with `modo_jobs` are scheduled in memory and are not persisted to the database. On restart, all cron schedules are re-registered from code. Do not design workflows that depend on cron state surviving a restart.

---

## docs.rs Quick Reference

| Type / Trait | Link |
|---|---|
| `AppBuilder` | https://docs.rs/modo/latest/modo/app/struct.AppBuilder.html |
| `AppState` | https://docs.rs/modo/latest/modo/app/struct.AppState.html |
| `ServiceRegistry` | https://docs.rs/modo/latest/modo/app/struct.ServiceRegistry.html |
| `Error` | https://docs.rs/modo/latest/modo/error/struct.Error.html |
| `HttpError` | https://docs.rs/modo/latest/modo/error/enum.HttpError.html |
| `HandlerResult` | https://docs.rs/modo/latest/modo/error/type.HandlerResult.html |
| `JsonResult` | https://docs.rs/modo/latest/modo/error/type.JsonResult.html |
| `ViewResult` | https://docs.rs/modo/latest/modo/error/type.ViewResult.html |
| `ErrorContext` | https://docs.rs/modo/latest/modo/error/struct.ErrorContext.html |
| `ErrorHandlerFn` | https://docs.rs/modo/latest/modo/error/type.ErrorHandlerFn.html |
| `GracefulShutdown` | https://docs.rs/modo/latest/modo/shutdown/trait.GracefulShutdown.html |
