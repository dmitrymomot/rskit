# modo v2 — Web Core Design Specification

## Overview

Plan 2 of the modo v2 multi-plan series. Builds on top of the foundation layer (Plan 1) to add: sanitization, validation, typed extractors, cookie management, and middleware. The result is a complete web core — handlers can receive typed, sanitized request bodies, validate input, and run behind a full middleware stack with unified error handling.

**Parent spec:** `docs/superpowers/specs/2026-03-19-modo-v2-design.md`
**Foundation plan:** `docs/superpowers/plans/2026-03-19-modo-v2-foundation.md`

## New Dependencies

```toml
# Add to Cargo.toml [dependencies]
axum-extra = { version = "0.12", features = ["cookie-signed", "cookie-private", "multipart"] }
tower_governor = { version = "0.8", default-features = false, features = ["axum"] }
regex = "1"
nanohtml2text = "0.2"

# Update tower-http features
tower-http = { version = "0.6", features = [
    "compression-full", "catch-panic", "trace",
    "cors", "request-id", "set-header", "sensitive-headers"
] }
```

## File Structure

```
src/
  sanitize/
    mod.rs                    -- mod + pub use
    traits.rs                 -- Sanitize trait
    functions.rs              -- trim, trim_lowercase, collapse_whitespace, strip_html, truncate, normalize_email
  validate/
    mod.rs                    -- mod + pub use
    traits.rs                 -- Validate trait
    error.rs                  -- ValidationError, From<ValidationError> for modo::Error (422)
    validator.rs              -- Validator builder, FieldValidator chain
    rules.rs                  -- rule implementations
  extractor/
    mod.rs                    -- mod + pub use
    service.rs                -- Service<T> (FromRequestParts)
    json.rs                   -- JsonRequest<T>
    form.rs                   -- FormRequest<T>
    query.rs                  -- Query<T> (with sanitize)
    multipart.rs              -- MultipartRequest<T>, UploadedFile, Files
  cookie/
    mod.rs                    -- mod + pub use
    config.rs                 -- CookieConfig
    key.rs                    -- Key management (from config secret, required)
  middleware/
    mod.rs                    -- mod + pub use
    request_id.rs             -- ULID X-Request-Id
    tracing.rs                -- structured request logging
    compression.rs            -- gzip/brotli/zstd
    catch_panic.rs            -- panic → modo::Error in response extensions
    security_headers.rs       -- configurable security headers + config
    cors.rs                   -- CORS config + origin strategies
    csrf.rs                   -- custom double-submit cookie
    rate_limit.rs             -- tower_governor wrapper + key extractors
    error_handler.rs          -- outermost response-rewriting middleware
tests/
  sanitize_test.rs
  validate_test.rs
  extractor_test.rs
  cookie_test.rs
  middleware_test.rs
```

## Build Order

Bottom-up by dependency:

1. sanitize (no deps on other modo modules)
2. validate (depends on error)
3. extractor (depends on service, sanitize)
4. cookie (depends on config, error)
5. middleware — simple wrappers first (request_id, tracing, compression, catch_panic, security_headers), then complex (cors, csrf, rate_limit, error_handler)

---

## Module 1: Sanitize

No dependencies on other modo modules. Pure string operations.

### Sanitize Trait

```rust
pub trait Sanitize {
    fn sanitize(&mut self);
}
```

Users implement manually — no derive macro. Extractors call `sanitize()` automatically on `T: Sanitize` before returning the value to the handler.

### Sanitizer Functions

All take `&mut String` and mutate in place:

| Function | Behavior |
|---|---|
| `trim(s)` | Strip leading/trailing whitespace |
| `trim_lowercase(s)` | Trim + lowercase |
| `collapse_whitespace(s)` | Multiple spaces/tabs/newlines → single space |
| `strip_html(s)` | Remove HTML tags, decode entities (via `nanohtml2text`) |
| `truncate(s, max_len)` | Truncate to max chars (not bytes), respects char boundaries |
| `normalize_email(s)` | Lowercase, strip plus-addressing (`user+tag@` → `user@`) |

### Usage

```rust
impl Sanitize for CreateTodo {
    fn sanitize(&mut self) {
        sanitize::trim(&mut self.title);
        sanitize::normalize_email(&mut self.email);
    }
}
```

---

## Module 2: Validate

Depends on: error module (for `From<ValidationError> for modo::Error`).

### ValidationError

```rust
pub struct ValidationError {
    fields: HashMap<String, Vec<String>>,
}
```

Implements `Display`, `std::error::Error`, and `From<ValidationError> for modo::Error` → 422 with JSON body:

```json
{
  "error": {
    "status": 422,
    "message": "validation failed",
    "fields": {
      "title": ["must be at least 3 characters"],
      "email": ["invalid email format"]
    }
  }
}
```

### Validate Trait

```rust
pub trait Validate {
    fn validate(&self) -> Result<(), ValidationError>;
}
```

Called explicitly by the user in handlers — not automatic. Form handlers need the original values for re-rendering on error, so validation is always opt-in.

### Validator Builder

```rust
impl Validate for CreateTodo {
    fn validate(&self) -> Result<(), ValidationError> {
        Validator::new()
            .field("title", &self.title)
                .required().min_length(3).max_length(100)
            .field("email", &self.email)
                .required().email()
            .field("count", &self.count)
                .range(1..=100)
            .check()
    }
}
```

The builder collects all errors across all fields and returns them together in `check()` — not fail-fast. Users see all validation problems at once.

### Validation Rules

On `FieldValidator`:

| Rule | Behavior |
|---|---|
| `required()` | Non-empty string / `Some` value |
| `min_length(n)` | String length >= n |
| `max_length(n)` | String length <= n |
| `email()` | Regex-based email validation |
| `url()` | Regex-based URL validation |
| `range(r)` | Numeric range (any `PartialOrd`) |
| `one_of(&[values])` | Value must be in list |
| `matches_regex(&str)` | Custom regex pattern |
| `custom(fn(&T) -> bool, "message")` | Arbitrary check |

---

## Module 3: Extractor

Depends on: service (AppState), sanitize (Sanitize trait), error (Error).

### Design Principle

modo extractors always require `T: DeserializeOwned + Sanitize` (or `T: Sanitize` for multipart). They deserialize + sanitize automatically. If the user doesn't want sanitization, they use axum's native extractors directly (`axum::Json<T>`, `axum::Form<T>`, `axum::extract::Query<T>`), which are already re-exported.

Validation is always explicit — `data.validate()?` in the handler.

### Service\<T\>

Reads `Arc<T>` from the registry via `State<AppState>`:

```rust
pub struct Service<T>(pub Arc<T>);
```

Implements `FromRequestParts`. Extracts from `State<AppState>` → calls `state.get::<T>()`. Returns 500 if `T` not found in registry.

Usage: `Service(db): Service<WritePool>`

### JsonRequest\<T\>

```rust
pub struct JsonRequest<T>(pub T);
// T: DeserializeOwned + Sanitize
```

Implements `FromRequest`:
1. Deserialize JSON body via `axum::Json<T>`
2. Call `value.sanitize()`
3. Return `JsonRequest(value)`

Returns 400 on deserialization failure.

### FormRequest\<T\>

```rust
pub struct FormRequest<T>(pub T);
// T: DeserializeOwned + Sanitize
```

Same as `JsonRequest` but uses `axum::Form<T>` internally. Content-Type: `application/x-www-form-urlencoded`.

### Query\<T\>

```rust
pub struct Query<T>(pub T);
// T: DeserializeOwned + Sanitize
```

Wraps `axum::extract::Query<T>`. Deserializes query string, then calls `sanitize()`.

### Path\<T\>

Re-export only — no sanitize needed:

```rust
pub use axum::extract::Path;
```

### MultipartRequest\<T\>

Automatically separates text fields from file fields:

```rust
pub struct MultipartRequest<T>(pub T, pub Files);

pub struct Files(HashMap<String, Vec<UploadedFile>>);

pub struct UploadedFile {
    pub name: String,
    pub content_type: String,
    pub size: usize,
    pub data: bytes::Bytes,
}
```

`Files` methods:
- `get(name) -> Option<&UploadedFile>` — first file for field name
- `get_all(name) -> &[UploadedFile]` — all files for field name
- `remove(name) -> Option<UploadedFile>` — take ownership of first file

Implements `FromRequest`:
1. Consume multipart stream
2. Text fields → key-value pairs → deserialize into `T` via serde
3. File fields → collect into `Files`
4. If `T: Sanitize`, call `sanitize()`
5. Return `MultipartRequest(value, files)`

`UploadedFile::from_field(field)` is a convenience method that reads name, content_type, size, and data from an axum multipart `Field`.

No `FromMultipart` trait. No proc macros. User defines a plain serde struct for text fields, accesses files by name:

```rust
#[derive(Deserialize)]
struct CreateProfile {
    name: String,
}

async fn create_profile(
    MultipartRequest(mut data, mut files): MultipartRequest<CreateProfile>,
) -> Result<Json<Profile>> {
    data.validate()?;
    let avatar = files.remove("avatar")
        .ok_or(Error::bad_request("avatar required"))?;
    // use data.name and avatar
}
```

---

## Module 4: Cookie

Depends on: config, error. Thin wrapper around `axum-extra`'s cookie jars.

### CookieConfig

```rust
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct CookieConfig {
    pub secret: String,           // required, 64+ char hex string — app fails on start if missing
    pub secure: bool,             // default: true
    pub http_only: bool,          // default: true
    pub same_site: String,        // "lax" | "strict" | "none", default: "lax"
    pub path: String,             // default: "/"
    pub domain: Option<String>,   // default: None
}
```

### Key Management

```rust
pub fn key_from_config(config: &CookieConfig) -> Result<Key> {
    if config.secret.len() < 64 {
        return Err(Error::internal("cookie secret must be at least 64 characters"));
    }
    Ok(Key::from(config.secret.as_bytes()))
}
```

No `Option<String>`, no `Key::generate()`. App will not start without an explicit secret. Users must set the secret in their config.

### Re-exports

```rust
pub use axum_extra::extract::cookie::{CookieJar, SignedCookieJar, PrivateCookieJar, Key};
```

### Config

```yaml
cookie:
  secret: ${COOKIE_SECRET}
  secure: true
  http_only: true
  same_site: lax
  path: /
```

---

## Module 5: Middleware

Depends on: error, id, config, cookie. Mix of thin ecosystem wrappers and custom implementations.

### Error Flow Protocol

Every modo middleware that short-circuits with an error status MUST store `modo::Error` in `response.extensions_mut()`. The error_handler middleware (outermost layer) reads this to provide unified error handling.

This is an internal protocol — not exposed to users. It enables a single error handler function to format all errors (handler errors, rate limit 429s, CSRF 403s, panics) with full request context for content negotiation.

### Middleware Ordering

Outermost (runs first on request) → innermost:

```rust
.layer(modo::middleware::error_handler(handler))      // 1. rewrites all 4xx/5xx
.layer(modo::middleware::catch_panic())                // 2. catches panics
.layer(modo::middleware::compression())                // 3. compress responses
.layer(modo::middleware::security_headers(&config))    // 4. security headers
.layer(modo::middleware::cors(&config))                // 5. CORS preflight
.layer(modo::middleware::request_id())                 // 6. generate request ID
.layer(modo::middleware::tracing())                    // 7. log request/response
.layer(modo::middleware::rate_limit(&config))          // 8. rate limit
.layer(modo::middleware::csrf(&config, &key))          // 9. CSRF protection
.layer(modo::session::layer(&registry))                // 10. session (Plan 3)
```

### request_id

Wraps tower-http `SetRequestIdLayer` + `PropagateRequestIdLayer`.

```rust
pub fn request_id() -> (SetRequestIdLayer, PropagateRequestIdLayer)
```

Implements `MakeRequestId` using `id::ulid()`. Sets `X-Request-Id` on request and propagates to response. ~15 lines.

### tracing

Wraps tower-http `TraceLayer`.

```rust
pub fn tracing() -> TraceLayer
```

Structured request logging: method, path, status, latency, request_id. ~20 lines.

### compression

Wraps tower-http `CompressionLayer`.

```rust
pub fn compression() -> CompressionLayer
```

Already in deps. gzip/brotli/zstd with automatic content negotiation.

### catch_panic

Wraps tower-http `CatchPanicLayer`.

```rust
pub fn catch_panic() -> CatchPanicLayer
```

Implements `ResponseForPanic` to produce a response with `modo::Error::internal("internal server error")` stored in response extensions. The error_handler then formats it based on request context. ~15 lines.

### security_headers

Stacks tower-http `SetResponseHeaderLayer`s.

```rust
pub struct SecurityHeadersConfig {
    pub x_content_type_options: bool,       // default: true → "nosniff"
    pub x_frame_options: String,            // default: "DENY"
    pub referrer_policy: String,            // default: "strict-origin-when-cross-origin"
    pub hsts_max_age: Option<u64>,          // default: None (enable in prod)
    pub content_security_policy: Option<String>,
    pub permissions_policy: Option<String>,
}

pub fn security_headers(config: &SecurityHeadersConfig) -> impl Layer
```

Stacks `SetResponseHeaderLayer::if_not_present()` for each enabled header. ~30 lines.

### cors

Wraps tower-http `CorsLayer`.

```rust
pub struct CorsConfig {
    pub origins: Vec<String>,
    pub methods: Vec<String>,              // default: ["GET","POST","PUT","DELETE","PATCH"]
    pub headers: Vec<String>,              // default: ["Content-Type","Authorization"]
    pub max_age_secs: u64,                 // default: 86400
    pub allow_credentials: bool,           // default: true
}

pub fn cors(config: &CorsConfig) -> CorsLayer
```

Origin strategies for dynamic validation:

```rust
pub mod cors {
    /// Static origin list check
    pub fn urls(origins: &[String]) -> impl Fn(&HeaderValue, &RequestParts) -> bool;
    /// Subdomain wildcard matching
    pub fn subdomains(domain: &str) -> impl Fn(&HeaderValue, &RequestParts) -> bool;
}
```

For DB-backed dynamic origins, user passes an async predicate directly to tower-http's `AllowOrigin::async_predicate()`. ~40 lines.

### csrf

Custom implementation. Double-submit cookie pattern with signed HttpOnly cookies.

```rust
pub struct CsrfConfig {
    pub cookie_name: String,               // default: "_csrf"
    pub header_name: String,               // default: "X-CSRF-Token"
    pub field_name: String,                // default: "_csrf_token"
    pub ttl_secs: u64,                     // default: 21600 (6 hours)
    pub exempt_methods: Vec<String>,       // default: ["GET","HEAD","OPTIONS"]
}

pub fn csrf(config: &CsrfConfig, cookie_key: &Key) -> CsrfLayer
```

Flow:

1. **Safe methods (GET/HEAD/OPTIONS):** Generate random token → store in signed HttpOnly cookie → inject token into request extensions (for template engine access via `{{ csrf_token() }}`)
2. **Unsafe methods (POST/PUT/DELETE):** Read token from signed cookie → compare with `X-CSRF-Token` header OR `_csrf_token` form field → reject with 403 if mismatch (store `modo::Error::forbidden(...)` in response extensions)
3. Middleware-level enforcement — no manual `verify()` in handlers

~100-150 lines.

### rate_limit

Wraps `tower_governor`.

```rust
pub struct RateLimitConfig {
    pub per_second: u64,                   // default: 1
    pub burst_size: u32,                   // default: 10
    pub use_headers: bool,                 // default: true (X-RateLimit-* headers)
    pub cleanup_interval_secs: u64,        // default: 60
}

/// Default rate limiter (by IP)
pub fn rate_limit(config: &RateLimitConfig) -> RateLimitBundle

/// Rate limiter with custom key extractor
pub fn rate_limit_with<K: KeyExtractor>(config: &RateLimitConfig, key: K) -> RateLimitBundle
```

`RateLimitBundle` holds the `GovernorLayer` and spawns `retain_recent()` cleanup automatically. Implements `Layer<S>` so it can be used directly with `.layer()`.

Built-in key extractors:

```rust
pub mod rate_limit {
    pub fn by_ip() -> PeerIpKeyExtractor;
    pub fn by_smart_ip() -> SmartIpKeyExtractor;
    pub fn by_header(name: &str) -> HeaderKeyExtractor;
}
```

Per-route limits: apply different `rate_limit()` calls with different configs to different sub-routers. Global + per-route limits stack — a request hits both limiters independently.

tower_governor's `.error_handler()` is wired to store `modo::Error` in response extensions so the error_handler can format 429 responses consistently. ~50 lines.

### error_handler

Custom response-rewriting middleware. Outermost layer.

```rust
pub fn error_handler<F, Fut>(handler: F) -> ErrorHandlerLayer<F>
where
    F: Fn(Error, &Request) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = Response> + Send;
```

After the inner middleware stack produces a response:
1. If status is 4xx or 5xx, read `modo::Error` from response extensions
2. Pass the error + original request to the user's handler function
3. User decides response format based on request context (Accept header, HX-Request, etc.)

If no `modo::Error` is in extensions (e.g., from a non-modo middleware), construct one from the response status code and body.

```rust
async fn my_error_handler(err: modo::Error, req: &Request) -> Response {
    let is_htmx = req.headers().contains_key("hx-request");
    let is_json = req.headers().get("accept")
        .map(|v| v.to_str().unwrap_or("").contains("application/json"))
        .unwrap_or(false);

    if is_htmx {
        Html("<div class=\"toast error\">...</div>").into_response()
    } else if is_json {
        (err.status(), Json(json!({"error": err.message()}))).into_response()
    } else {
        Html(render_error_page(&err)).into_response()
    }
}
```

~60 lines.

---

## Updated modo::Config

```rust
pub struct Config {
    pub server: server::Config,
    pub database: db::Config,
    pub tracing: tracing::Config,
    pub cookie: cookie::CookieConfig,
    pub security_headers: middleware::SecurityHeadersConfig,
    pub cors: middleware::CorsConfig,
    pub csrf: middleware::CsrfConfig,
    pub rate_limit: middleware::RateLimitConfig,
}
```

---

## Updated lib.rs

```rust
pub mod config;
pub mod cookie;
pub mod db;
pub mod error;
pub mod extractor;
pub mod id;
pub mod middleware;
pub mod runtime;
pub mod sanitize;
pub mod server;
pub mod service;
pub mod tracing;
pub mod validate;

pub use error::{Error, Result};
pub use config::Config;
```

---

## Full Bootstrap Example (after Plan 2)

```rust
use modo::{config, db, server, service, middleware};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = config::load::<AppConfig>("config/")?;
    modo::tracing::init(&config.modo.tracing)?;

    let cookie_key = modo::cookie::key_from_config(&config.modo.cookie)?;
    let pool = db::connect(&config.modo.database).await?;
    db::migrate("./migrations", &pool).await?;

    let mut registry = service::Registry::new();
    registry.add(pool.clone());

    let state = registry.into_state();
    let router = axum::Router::new()
        .nest("/api/todos", todo::routes())
        .nest("/auth", auth::routes()
            .layer(middleware::rate_limit(&strict_rate_limit).into_layer()))
        .layer(middleware::csrf(&config.modo.csrf, &cookie_key))
        .layer(middleware::rate_limit(&config.modo.rate_limit))
        .layer(middleware::tracing())
        .layer(middleware::request_id())
        .layer(middleware::cors(&config.modo.cors))
        .layer(middleware::security_headers(&config.modo.security_headers))
        .layer(middleware::compression())
        .layer(middleware::catch_panic())
        .layer(middleware::error_handler(my_error_handler))
        .with_state(state);

    let handle = server::http(router, &config.modo.server).await?;

    modo::runtime::run!(
        handle,
        db::managed(pool),
    ).await
}
```

---

## Summary

| Module | Key Types | New Dep |
|---|---|---|
| sanitize | `Sanitize` trait, 6 functions | nanohtml2text |
| validate | `Validate` trait, `ValidationError`, `Validator` builder, 9 rules | regex |
| extractor | `Service<T>`, `JsonRequest<T>`, `FormRequest<T>`, `Query<T>`, `MultipartRequest<T>`, `UploadedFile`, `Files` | axum-extra |
| cookie | `CookieConfig`, `key_from_config()`, re-exports | axum-extra |
| middleware | 9 middleware layers (request_id, tracing, compression, catch_panic, security_headers, cors, csrf, rate_limit, error_handler), 4 configs, error flow protocol | tower-http features, tower_governor |
