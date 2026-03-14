# Handlers Reference

Handlers are the core building block of a modo application. They are plain async functions
annotated with `#[modo::handler]` and discovered at compile time via `inventory`. No manual
route registration is required — attach the attribute and the route appears.

## Documentation

- modo crate: https://docs.rs/modo
- modo-macros crate: https://docs.rs/modo-macros

---

## Handler Registration

The `#[modo::handler(METHOD, "/path")]` attribute registers an async function as an HTTP route.
Registration happens at compile time; the route is submitted to the `inventory` collector and
wired automatically when `AppBuilder::run()` is called.

```rust
#[modo::handler(GET, "/")]
async fn index() -> &'static str {
    "Hello, modo!"
}
```

Supported HTTP methods (case-insensitive in the attribute, normalized to uppercase):

| Attribute token | HTTP method |
|----------------|-------------|
| `GET`          | GET         |
| `POST`         | POST        |
| `PUT`          | PUT         |
| `PATCH`        | PATCH       |
| `DELETE`       | DELETE      |
| `HEAD`         | HEAD        |
| `OPTIONS`      | OPTIONS     |

### Return Types

Handlers may return any type that implements `axum::response::IntoResponse`. Common patterns:

```rust
// Plain string
#[modo::handler(GET, "/ping")]
async fn ping() -> &'static str { "pong" }

// Fallible result with HTTP error
#[modo::handler(GET, "/item/{id}")]
async fn get_item(id: String) -> Result<String, modo::HttpError> {
    Err(modo::HttpError::NotFound)
}

// HandlerResult<T> — shorthand for Result<T, modo::Error>
#[modo::handler(GET, "/item/{id}")]
async fn get_item(id: String) -> modo::HandlerResult<String> {
    Ok(format!("item {id}"))
}

// JsonResult<T> — shorthand for Result<modo::Json<T>, modo::Error>
#[modo::handler(GET, "/items")]
async fn list_items() -> modo::JsonResult<Vec<String>> {
    Ok(modo::Json(vec!["a".into(), "b".into()]))
}
```

Use `modo::Json` (not `modo::axum::Json`) for JSON responses. `modo::HandlerResult<T>` and
`modo::JsonResult<T>` both accept an optional second type parameter for a custom error type.

---

## Modules

`#[modo::module(prefix = "/api")]` groups handlers under a shared URL prefix and optional
module-level middleware. Place it on a `mod` block containing `#[modo::handler]` functions.

```rust
#[modo::module(prefix = "/api/v1")]
mod api {
    #[modo::handler(GET, "/users")]
    async fn list_users() -> &'static str { "users" }

    #[modo::handler(POST, "/users")]
    async fn create_user() -> &'static str { "created" }
}
```

The module macro rewrites each inner `#[handler]` attribute to inject `module = "api"` so the
router knows which prefix applies. The final mounted paths become `/api/v1/users`, etc.

Module-level middleware applies to all routes in the module:

```rust
#[modo::module(prefix = "/admin", middleware = [require_admin])]
mod admin {
    #[modo::handler(GET, "/dashboard")]
    async fn dashboard() -> &'static str { "admin" }
}
```

Both `prefix` and `middleware` are named arguments. `prefix` is required; `middleware` is
optional and takes a bracket-enclosed comma-separated list.

---

## Path Parameters

Path parameters are declared with `{name}` syntax in the route path. The macro extracts them
automatically — declare only the parameters you need in the function signature; undeclared
parameters default to `String` and are ignored via the `..` destructuring pattern generated
inside the proc macro.

```rust
// Extract only `id`; any other path params would be ignored
#[modo::handler(GET, "/users/{id}")]
async fn get_user(id: String) -> String {
    format!("user {id}")
}

// Declare a typed path param
#[modo::handler(DELETE, "/todos/{id}")]
async fn delete_todo(id: String) -> modo::JsonResult<serde_json::Value> {
    Ok(modo::Json(serde_json::json!({"deleted": id})))
}

// Partial extraction: only declare params you need
#[modo::handler(GET, "/org/{org_id}/repo/{repo_id}/file/{name}")]
async fn get_file(name: String) -> String {
    // org_id and repo_id are present in the struct but ignored via `..`
    format!("file: {name}")
}
```

The macro generates a private `__HandlerNamePathParams` struct with all path parameters as
fields, using the declared type for named params and `String` for the rest. The handler
receives only the fields it declared.

Wildcard path parameters use `{*name}` syntax in the route path and capture the rest of the
URL segment.

---

## Extractors

Extractors appear as function parameters and are resolved by axum before the handler body runs.

### Query Parameters

Use `modo::extractors::QueryReq<T>` (re-export of `axum::extract::Query<T>`):

```rust
use modo::extractors::QueryReq;

#[derive(serde::Deserialize)]
struct Pagination {
    page: Option<u32>,
    per_page: Option<u32>,
}

#[modo::handler(GET, "/items")]
async fn list_items(QueryReq(pagination): QueryReq<Pagination>) -> String {
    format!("page {:?}", pagination.page)
}
```

### JSON Body

Two JSON types are available:

- `modo::Json<T>` (re-export of `axum::Json<T>`) — JSON **response** wrapper, no sanitization or validation.
- `modo::extractors::JsonReq<T>` — JSON **request** extractor that auto-sanitizes if `#[derive(Sanitize)]` is present on `T`, and exposes `.validate()` if `#[derive(Validate)]` is present.

Use `JsonReq<T>` for request extraction and `Json<T>` for responses:

```rust
use modo::extractors::JsonReq;
use modo::{Json, JsonResult};

#[modo::handler(POST, "/users")]
async fn create_user(body: JsonReq<CreateUser>) -> JsonResult<User> {
    body.validate()?;
    // body.0 gives the inner T; or use Deref: body.email
    Ok(Json(User { /* ... */ }))
}
```

### Form Data

`modo::extractors::FormReq<T>` deserializes `application/x-www-form-urlencoded`, auto-sanitizes,
and provides `.validate()`:

```rust
use modo::extractors::FormReq;

#[modo::handler(POST, "/contact")]
async fn contact(form: FormReq<ContactForm>) -> modo::HandlerResult<&'static str> {
    form.validate()?;
    Ok("submitted")
}
```

### Request ID

`modo::RequestId` extracts the per-request ULID injected by the request ID middleware. The ID
is read from or generated for the `X-Request-ID` header and propagated to the response:

```rust
#[modo::handler(GET, "/")]
async fn index(request_id: modo::RequestId) -> String {
    format!("request: {request_id}")
}
```

`RequestId` implements `Display` and exposes `.as_str() -> &str`.

### Client IP

`modo::middleware::ClientIp` provides the resolved client IP address, accounting for trusted
proxy headers (`CF-Connecting-IP`, `X-Real-IP`, `X-Forwarded-For`). It is populated by the
`client_ip_middleware` which reads trusted proxy CIDRs from the service registry:

```rust
use modo::middleware::ClientIp;

#[modo::handler(GET, "/whoami")]
async fn whoami(ClientIp(ip): ClientIp) -> String {
    ip.to_string()
}
```

`ClientIp` wraps `std::net::IpAddr` and implements `FromRequestParts<AppState>`.

### Rate Limit Info

`modo::middleware::RateLimitInfo` extracts rate limiting state injected into request extensions
by the rate limit middleware. Use it when you need to surface `X-RateLimit-*` values manually
or make decisions based on remaining quota:

```rust
use modo::middleware::RateLimitInfo;

#[modo::handler(GET, "/api/data")]
async fn get_data(rate: RateLimitInfo) -> String {
    format!("{}/{} remaining", rate.remaining, rate.limit)
}
```

Fields: `remaining: u32`, `limit: u32`, `reset_secs: u64`.

### Service Extractor

`modo::Service<T>` retrieves a registered service from the `ServiceRegistry` by
type. Returns `500 Internal Server Error` if the service is not registered. The inner `Arc<T>`
is accessible via `Deref` or by destructuring:

```rust
use modo::Service;

#[modo::handler(GET, "/status")]
async fn status(Service(cache): Service<MyCache>) -> String {
    cache.stats()
}
```

Register services in `main` via `app.service(my_value)` or `app.managed_service(my_value)`.

---

## Validation and Sanitization

### `#[derive(Validate)]`

Generates `impl modo::validate::Validate` with per-field rules. Call `.validate()` after
extracting to return a structured `400 Bad Request` on failure.

Field attribute: `#[validate(rule1, rule2, ...)]`

Available rules:

| Rule | Usage | Notes |
|------|-------|-------|
| `required` | `#[validate(required)]` | `None` or empty `String` fails |
| `min_length` | `#[validate(min_length = 5)]` | String length |
| `max_length` | `#[validate(max_length = 255)]` | String length |
| `email` | `#[validate(email)]` | Checks for `@` and `.` after `@` |
| `min` | `#[validate(min = 0)]` | Numeric minimum |
| `max` | `#[validate(max = 100)]` | Numeric maximum |
| `custom` | `#[validate(custom = "my_fn")]` | `fn(&T) -> Result<(), String>` |

Custom per-rule message: add `(message = "...")` after the rule value:

```rust
#[derive(serde::Deserialize, modo::Validate)]
struct SignupForm {
    #[validate(required(message = "Email is required"), email(message = "Invalid email"))]
    email: String,

    #[validate(required, min_length = 8, max_length = 72)]
    password: String,

    #[validate(min = 18, max = 120)]
    age: u32,
}
```

Field-level message (applied once across all rules for the field):

```rust
#[validate(required, email, message = "Provide a valid email")]
email: String,
```

### `#[derive(Sanitize)]`

Generates `impl modo::sanitize::Sanitize` and auto-registers it. Sanitization runs
automatically when data is extracted via `modo::extractors::JsonReq<T>` or `modo::extractors::FormReq<T>`.

Field attribute: `#[clean(rule1, rule2, ...)]`

Available rules:

| Rule | Effect |
|------|--------|
| `trim` | Remove leading/trailing whitespace |
| `lowercase` | Convert to lowercase |
| `uppercase` | Convert to uppercase |
| `strip_html_tags` | Remove HTML tags |
| `collapse_whitespace` | Replace multiple spaces with one |
| `truncate = N` | Truncate to N characters |
| `normalize_email` | Lowercase + strip `+tag` from local part (e.g. `user+tag@ex.com` → `user@ex.com`) |
| `custom = "fn_path"` | `fn(String) -> String` |

```rust
#[derive(serde::Deserialize, modo::Sanitize, modo::Validate)]
struct ContactForm {
    #[clean(trim, normalize_email)]
    #[validate(required, email)]
    email: String,

    #[clean(trim, strip_html_tags, truncate = 1000)]
    #[validate(required, min_length = 5, max_length = 1000)]
    message: String,
}
```

`Sanitize` only applies to named-field structs.

---

## Middleware

### Stacking Order

Global (outermost) → Module → Handler (innermost). A request passes through global middleware
first, then module middleware if the route belongs to a module, then handler middleware.

### Per-Handler Middleware

Use `#[middleware(fn_name)]` on the handler function. Bare paths are wrapped with
`axum::middleware::from_fn`; paths with arguments are called as layer factories:

```rust
#[modo::handler(GET, "/protected")]
#[middleware(require_auth)]
async fn protected() -> &'static str {
    "secret"
}

// Multiple middleware, applied outermost-first (last-declared = innermost)
#[modo::handler(POST, "/admin/action")]
#[middleware(require_auth, require_role("admin"))]
async fn admin_action() -> &'static str {
    "done"
}
```

### Module-Level Middleware

Pass `middleware = [...]` to `#[modo::module]`:

```rust
#[modo::module(prefix = "/api", middleware = [cors_layer, rate_limit_fn])]
mod api {
    // all routes here inherit the middleware
}
```

### Global Middleware via AppBuilder

```rust
app.layer(my_tower_layer)
   .cors(CorsConfig::with_origins(&["https://app.example.com"]))
   .rate_limit(RateLimitConfig { requests: 100, window_secs: 60 })
   .security_headers(SecurityHeadersConfig::default())
   .trailing_slash(TrailingSlash::Strip)
```

### CORS — `CorsConfig`

```rust
use modo::cors::{CorsConfig, CorsOrigins};

// Mirror request origin (default — permissive but not *)
let cors = CorsConfig::permissive();

// Fixed origin list
let cors = CorsConfig::with_origins(&["https://example.com", "https://app.example.com"]);

// Custom predicate
let cors = CorsConfig::with_custom_check(|origin| origin.ends_with(".example.com"));

// Fields
pub struct CorsConfig {
    pub origins: CorsOrigins,     // Any | List | Custom | Mirror
    pub credentials: bool,        // default: false
    pub max_age_secs: Option<u64>, // default: Some(3600)
}
```

CORS can also be set in YAML under `server.cors`:

```yaml
server:
  cors:
    origins: ["https://example.com"]
    credentials: false
    max_age_secs: 3600
```

### Rate Limiting — `RateLimitConfig`

Token-bucket rate limiting, applied globally by IP. Configured via `AppBuilder::rate_limit` or
under `server.rate_limit` in YAML:

```rust
use modo::config::RateLimitConfig;

app.rate_limit(RateLimitConfig {
    requests: 200,
    window_secs: 60,
})
```

```yaml
server:
  rate_limit:
    requests: 100
    window_secs: 60
```

Default: 100 requests per 60-second window. The middleware automatically sets
`X-RateLimit-Limit`, `X-RateLimit-Remaining`, `X-RateLimit-Reset`, and `Retry-After` headers.

### Security Headers — `SecurityHeadersConfig`

Configured via `AppBuilder::security_headers` or `server.security_headers` in YAML. Defaults
enable `X-Content-Type-Options: nosniff`, `X-Frame-Options: DENY`,
`Referrer-Policy: strict-origin-when-cross-origin`,
`Content-Security-Policy: default-src 'self'`, and HSTS (production only):

```rust
use modo::config::SecurityHeadersConfig;

app.security_headers(SecurityHeadersConfig {
    enabled: true,
    content_security_policy: Some("default-src 'self'; script-src 'nonce-...'".into()),
    ..Default::default()
})
```

```rust
pub struct SecurityHeadersConfig {
    pub enabled: bool,
    pub x_content_type_options: Option<String>,
    pub x_frame_options: Option<String>,
    pub referrer_policy: Option<String>,
    pub permissions_policy: Option<String>,
    pub content_security_policy: Option<String>,
    pub hsts: bool,          // only applied in Production environment
    pub hsts_max_age: u64,   // default: 31_536_000
}
```

### Trailing Slash — `TrailingSlash`

```rust
use modo::config::TrailingSlash;

app.trailing_slash(TrailingSlash::Strip)   // redirect /foo/ → /foo
// or
app.trailing_slash(TrailingSlash::Add)     // redirect /foo → /foo/
// or
app.trailing_slash(TrailingSlash::None)    // default: no modification
```

Configured in YAML under `server.http.trailing_slash` with values `none`, `strip`, or `add`.

### Catch Panic

Enabled by default (`catch_panic: true`). Converts handler panics into `500 Internal Server
Error` JSON responses and logs the panic message. Disable via `app.catch_panic(false)` or
`server.http.catch_panic: false` in YAML.

### Maintenance Mode

When enabled, all routes except `/_live` and `/_ready` return `503 Service Unavailable`.
Configure via `app.maintenance(true)` or `server.http.maintenance: true` in YAML. Set a custom
message with `server.http.maintenance_message`.

---

## Static Files

Two mutually exclusive features control static file serving:

| Feature | Use case | Cache |
|---------|----------|-------|
| `static-fs` | Development — serves from a filesystem directory | `max-age=3600` |
| `static-embed` | Production — embeds files at compile time via `rust-embed` | `max-age=31536000, immutable` + ETag/304 |

### `static-fs` (Development)

Enable the feature in `Cargo.toml`:

```toml
[dependencies]
modo = { version = "...", features = ["static-fs"] }
```

Configure in YAML (defaults shown):

```yaml
server:
  static_files:
    dir: "static"           # filesystem directory to serve
    prefix: "/static"       # URL prefix
    cache_control: null     # optional override, default: "max-age=3600"
```

`StaticConfig` fields: `dir: String`, `prefix: String`, `cache_control: Option<String>`.

### `static-embed` (Production)

Enable the feature:

```toml
[dependencies]
modo = { version = "...", features = ["static-embed"] }
```

Use the `static_assets` argument in `#[modo::main]` to embed a directory at compile time:

```rust
#[modo::main(static_assets = "static/")]
async fn main(
    app: modo::AppBuilder,
    config: modo::AppConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    app.config(config).run().await
}
```

The macro generates a `#[derive(rust_embed::Embed)] struct __ModoStaticAssets` with the folder
set to the provided path, then calls `app.embed_static_files::<__ModoStaticAssets>()`. The
default folder when `static_assets` is omitted is `"static/"`.

The embedded backend adds SHA-256-based ETags and returns `304 Not Modified` when
`If-None-Match` matches. Cache-Control defaults to `max-age=31536000, immutable`.

The URL prefix under which static files are served comes from `server.static_files.prefix` in the
config (default `/static`).

---

## Health Check Endpoints

`AppBuilder::run()` automatically mounts:

- `GET /_live` — liveness probe, always `200 OK`
- `GET /_ready` — readiness probe, runs all registered checks

Register async readiness checks:

```rust
app.readiness_check(|| async {
    db_ping().await.map_err(|e| Box::new(e) as _)
})
```

The liveness and readiness paths can be configured via `server.liveness_path` and
`server.readiness_path` in YAML (defaults: `/_live`, `/_ready`).

---

## Integration Patterns

### Static Files + Templates

When both `templates` and `static-embed` (or `static-fs`) features are enabled, register
static files before calling `run()`. The template engine is auto-registered as a service when
`modo-templates` is wired in; no manual `.layer()` call is needed for template rendering.

For embedded assets in production:

```rust
#[modo::main(static_assets = "static/")]
async fn main(
    app: modo::AppBuilder,
    config: modo::AppConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    app.config(config).run().await
}
```

### JSON API with Validation

```rust
use modo::extractors::JsonReq;
use modo::{Json, JsonResult};

#[derive(serde::Deserialize, modo::Sanitize, modo::Validate)]
struct CreateTodo {
    #[clean(trim)]
    #[validate(required, min_length = 1, max_length = 255)]
    title: String,
}

#[modo::handler(POST, "/todos")]
async fn create_todo(input: JsonReq<CreateTodo>) -> JsonResult<TodoResponse> {
    input.validate()?;
    // ...
    Ok(Json(TodoResponse { /* ... */ }))
}
```

### Grouped API Module with Rate Limit

```rust
#[modo::module(prefix = "/api/v1", middleware = [api_rate_limit_layer])]
mod api_v1 {
    #[modo::handler(GET, "/users")]
    async fn list_users() -> modo::JsonResult<Vec<User>> { /* ... */ }

    #[modo::handler(GET, "/users/{id}")]
    async fn get_user(id: String) -> modo::JsonResult<User> { /* ... */ }
}
```

---

## Gotchas

- **`modo::Json` vs `modo::extractors::JsonReq`**: `modo::Json` is `axum::Json` — it is the **response** wrapper with no sanitization.
  Use `modo::extractors::JsonReq<T>` for request extraction with auto-sanitization. `modo::extractors::FormReq<T>` similarly auto-sanitizes forms.

- **Path param partial extraction**: Declare only the params you need in the function
  signature. The macro generates a struct with all params; missing ones default to `String`
  and are discarded via `..`. You do not need `axum::extract::Path` manually.

- **Middleware stacking order**: Global layers (via `app.layer(...)`) are outermost. Module
  middleware wraps the module router. Handler middleware (via `#[middleware(...)]`) is innermost
  via `route_layer`. A request encounters global → module → handler layers in that order.

- **`inventory` linking in tests**: Handlers registered via `inventory::submit!` may not link
  in integration tests without a direct use. Force linking with `use crate::handlers::my_handler as _;`
  if routes disappear in tests.

- **`static-embed` requires the feature**: `#[modo::main(static_assets = "...")]` is a
  compile error if the `static-embed` feature is not enabled.

- **`#[derive(Sanitize)]` auto-registers globally**: The macro submits a `SanitizerRegistration`
  to `inventory`. This means sanitization applies automatically via `modo::extractors::JsonReq` and
  `modo::extractors::FormReq` without any explicit call in the handler.

- **`TrailingSlash::Strip`/`Add` issues 301 redirects**: This means POST bodies are lost on
  redirect. Prefer consistent URL shapes in your API rather than relying on redirect normalization.

- **`RateLimitInfo` extractor requires the middleware**: Extracting `RateLimitInfo` in a
  handler fails with a 500 if the rate limit middleware is not configured. Guard with
  `Option<RateLimitInfo>` if the middleware is optional.

---

## Key Type Reference

| Type | Crate path |
|------|-----------|
| `AppBuilder` | `modo::AppBuilder` |
| `AppState` | `modo::AppState` |
| `ServiceRegistry` | `modo::ServiceRegistry` |
| `CorsConfig` | `modo::cors::CorsConfig` |
| `CorsOrigins` | `modo::cors::CorsOrigins` |
| `RateLimitConfig` | `modo::config::RateLimitConfig` |
| `RateLimitInfo` | `modo::middleware::RateLimitInfo` |
| `SecurityHeadersConfig` | `modo::config::SecurityHeadersConfig` |
| `TrailingSlash` | `modo::config::TrailingSlash` |
| `HttpConfig` | `modo::config::HttpConfig` |
| `StaticConfig` | `modo::static_files::StaticConfig` (`pub(crate)` — access via `AppConfig.server.static_files`) |
| `ClientIp` | `modo::middleware::ClientIp` |
| `RequestId` | `modo::RequestId` |
| `Service<T>` | `modo::Service` |
| `Json<T>` (response) | `modo::Json` |
| `JsonReq<T>` (request, sanitizing) | `modo::extractors::JsonReq` |
| `FormReq<T>` (request, sanitizing) | `modo::extractors::FormReq` |
| `QueryReq<T>` (request) | `modo::extractors::QueryReq` |
| `PathReq<T>` (request) | `modo::extractors::PathReq` |
| `HandlerResult<T>` | `modo::HandlerResult` |
| `JsonResult<T>` | `modo::JsonResult` |
