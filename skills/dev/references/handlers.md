# Handlers, Routing, Middleware, and Server

## Plain Async Function Handlers

Handlers in modo are plain `async fn` -- no macros, no attribute annotations, no signature rewriting. Any async function that satisfies axum's `Handler` trait works directly:

```rust
use axum::Json;
use modo::Service;

// A handler is just an async function.
async fn list_items(Service(db): Service<DbPool>) -> Json<Vec<Item>> {
    let items = db.fetch_all().await;
    Json(items)
}

async fn get_item(
    axum::extract::Path(id): axum::extract::Path<String>,
    Service(db): Service<DbPool>,
) -> modo::Result<Json<Item>> {
    let item = db.find(&id).await.map_err(|_| modo::Error::not_found("item not found"))?;
    Ok(Json(item))
}
```

Extractors are function parameters. modo provides:

- `Service<T>` -- retrieves `Arc<T>` from the service registry
- `modo::extractor::JsonRequest<T>` / `modo::extractor::FormRequest<T>` -- deserialize + sanitize request bodies (`T: Sanitize`)
- `axum::extract::Path<T>` / `axum::extract::Query<T>` -- path and query parameters
- `ClientIp` -- resolved client IP (requires `ClientIpLayer`)
- `Session` -- session data
- `Flash` -- flash messages
- `Role` -- RBAC role (requires RBAC middleware)

Return types: `Json<T>`, `Html<String>`, `axum::response::Redirect`, `axum::response::Response`, or `modo::Result<T>` for fallible handlers.

Error constructors (all take `msg: impl Into<String>`): `Error::not_found(msg)`, `Error::bad_request(msg)`, `Error::internal(msg)`, `Error::unauthorized(msg)`, `Error::forbidden(msg)`, `Error::too_many_requests(msg)`, `Error::conflict(msg)`, `Error::unprocessable_entity(msg)`.

## Routing with axum::Router

Routes use `axum::Router` directly. modo re-exports axum as `modo::axum`.

```rust
use modo::axum::{Router, routing::{get, post, put, delete}};
use modo::service::{Registry, AppState};

let mut registry = Registry::new();
registry.add(my_db_pool);
registry.add(my_email_client);
let state: AppState = registry.into_state();

let app = Router::new()
    .route("/items", get(list_items).post(create_item))
    .route("/items/{id}", get(get_item).put(update_item).delete(delete_item))
    .with_state(state);
```

The service registry (`Registry`) is a `HashMap<TypeId, Arc<dyn Any + Send + Sync>>`. Call `.add(value)` to insert, then `.into_state()` to freeze into `AppState`. Inside handlers, `Service<T>` extracts the registered value.

## Middleware

All middleware functions return Tower-compatible layers. Apply them with `.layer()` on the router.

### Recommended Layer Order

Outermost (applied first to request, last to response) to innermost:

```rust
use modo::middleware;
use tokio_util::sync::CancellationToken;

let cancel = CancellationToken::new();

let app = Router::new()
    .route("/", get(handler))
    .with_state(state)
    .layer(middleware::error_handler(render_error))
    .layer(middleware::tracing())
    .layer(middleware::request_id())
    .layer(middleware::catch_panic())
    .layer(middleware::compression())
    .layer(middleware::cors(&cors_config))
    .layer(middleware::security_headers(&sec_config)?)
    .layer(middleware::rate_limit(&rl_config, cancel.clone()))
    .layer(ClientIpLayer::new());
```

### Rate Limiting

Token-bucket algorithm. Each key gets `burst_size` tokens; tokens replenish at `per_second` rate. Exhausted buckets receive `429 Too Many Requests`.

**Configuration (`RateLimitConfig`):** `#[non_exhaustive]`, `#[serde(default)]` -- construct via `Default` or deserialize.

```rust
use modo::middleware::RateLimitConfig;

// #[non_exhaustive] -- use Default + field mutation (struct literals won't compile outside the crate)
let mut config = RateLimitConfig::default();
config.per_second = 1;          // token replenish rate (default: 1)
config.burst_size = 10;         // max tokens per key (default: 10)
config.use_headers = true;      // include x-ratelimit-* headers (default: true)
config.cleanup_interval_secs = 60; // (default: 60)
config.max_keys = 10_000;       // 0 = unlimited (default: 10_000)
```

**IP-based rate limiting (`rate_limit`):**

```rust
use modo::middleware::{rate_limit, RateLimitConfig};
use tokio_util::sync::CancellationToken;

let cancel = CancellationToken::new();
let layer = rate_limit(&config, cancel.clone());
```

Requires the server to expose `ConnectInfo<SocketAddr>` (modo's `server::http()` calls `into_make_service()` which does NOT set this up -- use `into_make_service_with_connect_info::<SocketAddr>()` if you need `PeerIpKeyExtractor`).

**Custom key extraction (`rate_limit_with`):**

```rust
use modo::middleware::{rate_limit_with, KeyExtractor, GlobalKeyExtractor, RateLimitConfig};

// Global shared bucket (all requests share one limit)
let layer = rate_limit_with(&config, GlobalKeyExtractor, cancel.clone());
```

Implement `KeyExtractor` for custom keys:

```rust
use modo::middleware::KeyExtractor;
use http::Request;

#[derive(Clone)]
struct ApiKeyExtractor;

impl KeyExtractor for ApiKeyExtractor {
    fn extract<B>(&self, req: &Request<B>) -> Option<String> {
        req.headers()
            .get("x-api-key")
            .and_then(|v| v.to_str().ok())
            .map(String::from)
    }
}
```

Both `rate_limit()` and `rate_limit_with()` spawn a background cleanup task. The `CancellationToken` must be cancelled during shutdown to stop it.

Response headers when `use_headers` is true: `x-ratelimit-limit`, `x-ratelimit-remaining`, `x-ratelimit-reset`. Rejected responses also include `retry-after`.

Built-in extractors:

- `PeerIpKeyExtractor` -- keys by peer socket IP (needs `ConnectInfo<SocketAddr>`)
- `GlobalKeyExtractor` -- single shared bucket for all requests

`RateLimitLayer<K>` is also re-exported from `modo::middleware` -- it is the concrete Tower layer type returned by `rate_limit()` and `rate_limit_with()`. Typically you don't name this type directly; just use `let layer = rate_limit(...)` or `rate_limit_with(...)`.

### Tracing

Creates an `http_request` span per request with `method`, `uri`, `version`, and an empty `tenant_id` field (filled later by tenant middleware).

```rust
let layer = modo::middleware::tracing();
```

Built on `tower_http::trace::TraceLayer` with a custom `ModoMakeSpan`.

### CORS

**Static origins (`cors`):** `CorsConfig` is `#[non_exhaustive]` with `#[serde(default)]`.

```rust
use modo::middleware::{cors, CorsConfig};

// #[non_exhaustive] -- use Default + field mutation
let mut config = CorsConfig::default();
config.origins = vec!["https://example.com".into()];
// defaults: methods=["GET","POST","PUT","DELETE","PATCH"],
//   headers=["Content-Type","Authorization"], max_age_secs=86400,
//   allow_credentials=true
let layer = cors(&config);
```

When `origins` is empty, allows any origin (`*`) and forces `allow_credentials` to `false` (CORS spec requirement).

**Dynamic origins (`cors_with`):**

```rust
use modo::middleware::{cors_with, subdomains, urls, CorsConfig};

// Match any subdomain of example.com
let layer = cors_with(&config, subdomains("example.com"));

// Match exact URL list
let layer = cors_with(&config, urls(&["https://app.example.com".into()]));
```

Built-in predicates:

- `subdomains(domain)` -- matches the domain and any subdomain (both http and https)
- `urls(origins)` -- exact-match against a list of origin strings

### Compression

Gzip, deflate, brotli, zstd based on `Accept-Encoding`:

```rust
let layer = modo::middleware::compression();
```

### Request ID

Sets and propagates `x-request-id` header. Preserves incoming value; generates a ULID if absent:

```rust
let layer = modo::middleware::request_id();
```

### Catch Panic

Converts handler panics into 500 responses. Stores a `modo::Error` in response extensions for `error_handler` to intercept:

```rust
let layer = modo::middleware::catch_panic();
```

### Error Handler

Centralised error-response rendering. Intercepts any response that has a `modo::Error` in its extensions (set by `Error::into_response()`, `catch_panic`, `csrf`, `rate_limit`, etc.):

```rust
use modo::middleware::error_handler;

async fn render_error(err: modo::Error, parts: http::request::Parts) -> axum::response::Response {
    use axum::response::IntoResponse;
    (err.status(), err.message().to_string()).into_response()
}

let layer = error_handler(render_error);
```

The handler receives the error and the original request `Parts` (method, URI, headers, extensions).

### Security Headers

Adds security response headers. `SecurityHeadersConfig` is `#[non_exhaustive]` with `#[serde(default)]`.

```rust
use modo::middleware::{security_headers, SecurityHeadersConfig};

// #[non_exhaustive] -- use Default + field mutation
let mut config = SecurityHeadersConfig::default();
// defaults: x_content_type_options=true, x_frame_options="DENY",
//   referrer_policy="strict-origin-when-cross-origin",
//   hsts_max_age=None, content_security_policy=None, permissions_policy=None
config.hsts_max_age = Some(31536000);       // enable Strict-Transport-Security
let layer = security_headers(&config)?;
```

### CSRF

Double-submit signed-cookie pattern. Exempt methods (GET, HEAD, OPTIONS by default) generate a token; unsafe methods must echo the token via the configured header. `CsrfConfig` is `#[non_exhaustive]` with `#[serde(default)]`.

```rust
use modo::middleware::{csrf, CsrfConfig};
use modo::cookie::Key;

// #[non_exhaustive] -- use Default + field mutation
let mut config = CsrfConfig::default();
// defaults: cookie_name="_csrf", header_name="X-CSRF-Token",
//   field_name="_csrf_token", ttl_secs=21600,
//   exempt_methods=["GET","HEAD","OPTIONS"]
config.ttl_secs = 3600; // override if needed
let key = Key::generate();
let layer = csrf(&config, &key);
```

`CsrfToken` is a tuple struct (`pub struct CsrfToken(pub String)`, derives `Clone, Debug`) inserted into request/response extensions for handler/template access. Access the inner token string via `.0`.

Note: `field_name` is retained for configuration compatibility but is **not currently validated** by the middleware -- token validation is header-only. Form-field-based token submission is not supported.

## ClientIp Extraction

### ClientIp Extractor

`ClientIp` is an axum extractor that reads the resolved IP from request extensions (inserted by `ClientIpLayer`):

```rust
use modo::ClientIp;

async fn handler(ClientIp(ip): ClientIp) -> String {
    ip.to_string()
}
```

Returns `Error::internal` if `ClientIpLayer` is not applied.

### ClientIpLayer

Tower layer that resolves the real client IP on every request.

Resolution order:

1. If `trusted_proxies` is non-empty and the connecting IP is NOT in any trusted range, return the connecting IP directly (ignore proxy headers).
2. `X-Forwarded-For` header -- first valid IP.
3. `X-Real-IP` header -- valid IP.
4. `ConnectInfo<SocketAddr>` as fallback.
5. `127.0.0.1` if nothing is available.

```rust
use modo::ClientIpLayer;

// No trusted proxies -- headers trusted unconditionally
let layer = ClientIpLayer::new();
// Equivalent: ClientIpLayer implements Default
let layer = ClientIpLayer::default();

// With trusted proxy CIDR ranges
let trusted: Vec<ipnet::IpNet> = vec!["10.0.0.0/8".parse().unwrap()];
let layer = ClientIpLayer::with_trusted_proxies(trusted);
```

The `trusted_proxies` field is a top-level config value (not under `session`), parsed into `Vec<IpNet>` at startup.

### extract_client_ip()

Standalone utility function that resolves the real client IP from headers and connection info. This is the same logic used internally by `ClientIpLayer`, exposed for cases where you need IP resolution outside of middleware (e.g., in a custom extractor or service).

```rust
use modo::ip::extract_client_ip;
use http::HeaderMap;
use std::net::IpAddr;

let headers = HeaderMap::new();
let trusted: Vec<ipnet::IpNet> = vec!["10.0.0.0/8".parse().unwrap()];
let connect_ip: Option<IpAddr> = Some("10.0.0.1".parse().unwrap());

let real_ip: IpAddr = extract_client_ip(&headers, &trusted, connect_ip);
```

**Signature:** `pub fn extract_client_ip(headers: &HeaderMap, trusted_proxies: &[ipnet::IpNet], connect_ip: Option<IpAddr>) -> IpAddr`

Resolution order is identical to `ClientIpLayer` (see above). Not re-exported at the `modo::` top level -- access via `modo::ip::extract_client_ip`.

## Server Configuration and Graceful Shutdown

### Config

`Config` is `#[non_exhaustive]` with `#[serde(default)]` -- always construct via `Config::default()` and then mutate fields, or deserialize from YAML.

```rust
use modo::server::Config;

// Defaults: host="localhost", port=8080, shutdown_timeout_secs=30
let config = Config::default();
```

YAML:

```yaml
server:
    host: 0.0.0.0
    port: ${PORT:8080}
    shutdown_timeout_secs: 30
```

### Starting the Server

`modo::server::http()` binds a TCP listener and returns an `HttpServer` (a public struct) that implements `Task`:

```rust
use modo::server::{Config, http};

#[tokio::main]
async fn main() -> modo::Result<()> {
    let config = Config::default();
    let app = modo::axum::Router::new()
        .route("/", modo::axum::routing::get(health))
        .with_state(state);
    let server = http(app, &config).await?;
    modo::run!(server).await
}

async fn health() -> &'static str { "ok" }
```

### Graceful Shutdown

The `run!` macro orchestrates shutdown:

1. Waits for `SIGINT` (Ctrl+C) or `SIGTERM`.
2. Calls `Task::shutdown()` on each task in declaration order.
3. `HttpServer::shutdown()` signals the server to stop accepting connections, then waits up to `shutdown_timeout_secs` for in-flight requests to drain.

```rust
// Multiple tasks shut down in order
modo::run!(worker, server).await
```

Implement `Task` for custom services:

```rust
use modo::runtime::Task;

struct MyWorker { /* ... */ }

impl Task for MyWorker {
    async fn shutdown(self) -> modo::Result<()> {
        // cleanup logic
        Ok(())
    }
}
```

### wait_for_shutdown_signal

For finer-grained control over the shutdown sequence (instead of using `run!`), use `wait_for_shutdown_signal` directly:

```rust
use modo::runtime::wait_for_shutdown_signal;

wait_for_shutdown_signal().await;
// perform custom cleanup...
```

Resolves on `SIGINT` (Ctrl+C) or `SIGTERM` (Unix only). Panics if the OS signal handler cannot be installed.

## Gotchas

**Handler functions inside `#[tokio::test]` closures do not satisfy axum's `Handler` bounds.** Define test handler functions at module level (outside the test function):

```rust
// WRONG -- won't compile
#[tokio::test]
async fn test_route() {
    async fn handler() -> &'static str { "ok" }  // not a valid Handler
    let app = Router::new().route("/", get(handler));
}

// CORRECT
async fn handler() -> &'static str { "ok" }

#[tokio::test]
async fn test_route() {
    let app = Router::new().route("/", get(handler));
}
```

**`Router::layer()` bounds require `+ Sync`.** Both the layer `L` and its produced service `L::Service` must be `Send + Sync`, and the error type must be `Into<Infallible>` (not `Into<Box<dyn Error>>`).

**`PathParamStrategy` requires `.route_layer()` not `.layer()`.** Path parameters only exist after route matching, so layers that depend on them must be applied with `route_layer()`.

**`server::http()` uses `into_make_service()` (not `into_make_service_with_connect_info`).** `PeerIpKeyExtractor` for rate limiting requires `ConnectInfo<SocketAddr>`, which is only available when the server is started with `into_make_service_with_connect_info::<SocketAddr>()`. If you use modo's built-in `server::http()`, prefer `ClientIpLayer` + a custom `KeyExtractor` that reads `ClientIp` from extensions instead.
