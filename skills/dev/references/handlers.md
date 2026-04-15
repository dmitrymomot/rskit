# Handlers, Routing, Middleware, and Server

## Plain Async Function Handlers

Handlers in modo are plain `async fn` -- no macros, no attribute annotations, no signature rewriting. Any async function that satisfies axum's `Handler` trait works directly:

```rust
use axum::Json;
use modo::service::Service;

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

Extractors are function parameters. The usual ones are preluded (see `modo::prelude` below); the full flat index lives at `modo::extractors`. Commonly used:

- `Service<T>` (from `modo::service`) -- retrieves `Arc<T>` from the service registry
- `JsonRequest<T>` / `FormRequest<T>` / `MultipartRequest<T>` (from `modo::extractor`, re-exported via `modo::extractors`) -- deserialize + sanitize request bodies (`T: Sanitize`)
- `UploadedFile`, `Query<T>` (from `modo::extractor`)
- `axum::extract::Path<T>` -- path parameters
- `ClientIp` -- resolved client IP (requires `ClientIpLayer`)
- `ClientInfo` -- structured client metadata (IP, user-agent, fingerprint)
- `Session` -- session data
- `Flash` -- flash messages
- `Role` -- RBAC role (requires RBAC middleware)
- `Tenant`, `TenantId` -- multi-tenant identity
- `Bearer`, `Claims` -- JWT bearer token / claims
- `ApiKeyMeta` -- API key metadata
- `LastEventId` -- SSE reconnection cursor
- `HxRequest` -- HTMX request marker
- `TierInfo` -- resolved tier information
- `AppState` -- shared application state handle
- `MatchedHost` -- subdomain matched by a `HostRouter` wildcard pattern

Return types: `Json<T>`, `Html<String>`, `axum::response::Redirect`, `axum::response::Response`, or `modo::Result<T>` for fallible handlers.

Error constructors (all take `msg: impl Into<String>`): `Error::not_found(msg)`, `Error::bad_request(msg)`, `Error::internal(msg)`, `Error::unauthorized(msg)`, `Error::forbidden(msg)`, `Error::too_many_requests(msg)`, `Error::conflict(msg)`, `Error::unprocessable_entity(msg)`.

### `modo::prelude`

Handler-time prelude: `use modo::prelude::*;` brings in the ambient types reached for on nearly every request. Contents:

- `Error`, `Result` -- framework error type and alias
- `AppState` -- shared application state handle
- `Role`, `Session` -- RBAC and session extractors (from `auth`)
- `Flash` -- per-request flash messages
- `ClientIp` -- resolved client IP extractor
- `Tenant`, `TenantId` -- multi-tenant extractor and identifier
- `Validate`, `ValidationError`, `Validator` -- request-body validation trait, error, fluent helper

Less-universal extractors (JWT claims, OAuth providers, API keys, mailer, templates, jobs, storage, SSE, etc.) are intentionally NOT preluded -- import explicitly from `modo::extractors::*` or the feature module where used.

### `modo::extractors`

Flat index of every axum extractor modo ships. Re-exports (for `use modo::extractors::*;` or `modo::extractors::JsonRequest`):

- `FormRequest`, `JsonRequest`, `MultipartRequest`, `Query`, `UploadedFile` (from `extractor`)
- `ApiKeyMeta` (from `auth::apikey`)
- `Bearer`, `Claims` (from `auth::session::jwt`)
- `Role`, `Session` (from `auth::role`, `auth::session`)
- `Flash` (from `flash`)
- `ClientInfo`, `ClientIp` (from `ip`)
- `AppState` (from `service`)
- `LastEventId` (from `sse`)
- `HxRequest` (from `template`)
- `Tenant` (from `tenant`)
- `TierInfo` (from `tier`)

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

The service registry (`Registry`) is a `HashMap<TypeId, Arc<dyn Any + Send + Sync>>`. Call `.add(value)` to insert, then `.into_state()` to freeze into `AppState`. Inside handlers, `modo::service::Service<T>` extracts the registered value.

## Middleware

All middleware functions return Tower-compatible layers. Apply them with `.layer()` on the router.

### `modo::middlewares` (flat index)

`modo::middlewares` is the virtual wiring-site index. Typical use: `use modo::middlewares as mw;` then `.layer(mw::cors(...))`, `.layer(mw::Jwt::new(cfg))`, etc.

**Two calling conventions** -- this reflects upstream constructor design in each domain module:

- **lower_case names are free functions** -- call directly:
    - `mw::role(extractor)` (from `auth::role::middleware`)
    - `mw::tenant(strategy, resolver)` (from `tenant::middleware`)
    - Always-available universals: `mw::catch_panic()`, `mw::compression()`, `mw::cors(&cfg)`, `mw::csrf(&cfg, &key)`, `mw::error_handler(f)`, `mw::rate_limit(&cfg, cancel)`, `mw::request_id()`, `mw::security_headers(&cfg)?`, `mw::tracing()`
- **PascalCase names are `Layer` structs** -- call `::new(...)`:
    - `mw::Jwt::new(cfg)` (aliases `auth::session::jwt::JwtLayer`)
    - `mw::ApiKey::new(store)` (aliases `auth::apikey::ApiKeyLayer`)
    - `mw::Flash::new(cookie_cfg)` (aliases `flash::FlashLayer`)
    - `mw::Geo::new(...)` (aliases `geolocation::GeoLayer`)
    - `mw::ClientIp::new(trusted_proxies)` (aliases `ip::ClientIpLayer`)
    - `mw::TemplateContext::new(...)` (aliases `template::TemplateContextLayer`)
    - `mw::Tier::new(...)` (aliases `tier::TierLayer`)

> **v0.8 note:** The `session` free function (`mw::session(...)`) was removed. Session middleware is now constructed via `CookieSessionService::layer()` -- see `modo::auth::session::cookie::CookieSessionService`. The session layer is not re-exported from `modo::middlewares`.

The underlying `modo::middleware` module (singular) ships only the universal always-available layers (CORS, CSRF, compression, request-id, tracing, catch-panic, error-handler, security-headers, rate-limit) along with their configs and supporting types (`CorsConfig`, `CsrfConfig`, `CsrfToken`, `RateLimitConfig`, `RateLimitLayer`, `SecurityHeadersConfig`, `KeyExtractor`, `PeerIpKeyExtractor`, `GlobalKeyExtractor`, predicates `subdomains`/`urls`).

### Recommended Layer Order

Outermost (applied first to request, last to response) to innermost:

```rust
use modo::middleware;
use modo::ip::ClientIpLayer;
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

(The same layers are reachable from `modo::middlewares` as `mw::cors`, `mw::tracing`, ..., and `mw::ClientIp::new()`.)

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
use modo::ip::ClientIp; // also: use modo::prelude::*;

async fn handler(ClientIp(ip): ClientIp) -> String {
    ip.to_string()
}
```

Returns `Error::internal` if `ClientIpLayer` is not applied.

### ClientInfo Extractor

`ClientInfo` aggregates `ip`, `user_agent`, and `fingerprint` (from the `x-fingerprint` header) in one extractor. Implements `FromRequestParts`. The `ip` field falls through to `None` if `ClientIpLayer` is not applied -- `ClientInfo` does not itself fail.

```rust
use modo::ip::ClientInfo;

async fn handler(info: ClientInfo) -> String {
    format!(
        "ip={:?} ua={:?} fp={:?}",
        info.ip_value(),
        info.user_agent_value(),
        info.fingerprint_value(),
    )
}
```

Outside an HTTP request (background jobs, CLI tools), build manually with the chainable setters:

```rust
let info = ClientInfo::new()
    .ip("1.2.3.4")
    .user_agent("my-script/1.0")
    .fingerprint("abc123");
```

Fields are private; read with `ip_value()` / `user_agent_value()` / `fingerprint_value()` (each returns `Option<&str>`).

### ClientIpLayer

Tower layer that resolves the real client IP on every request.

Resolution order:

1. If `trusted_proxies` is non-empty and the connecting IP is NOT in any trusted range, return the connecting IP directly (ignore proxy headers).
2. `X-Forwarded-For` header -- first valid IP.
3. `X-Real-IP` header -- valid IP.
4. `ConnectInfo<SocketAddr>` as fallback.
5. `127.0.0.1` if nothing is available.

```rust
use modo::ip::ClientIpLayer;

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

`modo::server::http()` binds a TCP listener and returns an `HttpServer` (a public struct) that implements `Task`. Accepts `impl Into<axum::Router>`, so both a plain `axum::Router` and a `HostRouter` can be passed directly:

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

### Host-Based Routing

`HostRouter` dispatches requests to different `axum::Router`s based on the `Host` header. Supports exact matches, single-level wildcard subdomains (`*.acme.com`), and an optional fallback. Both use `HashMap` lookups for O(1) matching.

```rust
use modo::server::{self, Config, HostRouter};

let app = HostRouter::new()
    .host("acme.com", landing_router)
    .host("app.acme.com", admin_router)
    .host("*.acme.com", tenant_router)
    .fallback(not_found_router);

let server = server::http(app, &config).await?;
```

Builder methods panic on misconfiguration (duplicates, invalid wildcards like `*.com`). Complete all route registration before passing the router to `http()` or cloning it.

**`MatchedHost` extractor** -- available in handlers on wildcard-matched routes. Contains `subdomain` (e.g. `"tenant1"`) and `pattern` (e.g. `"*.acme.com"`):

```rust,ignore
use modo::server::MatchedHost;

async fn handler(matched: MatchedHost) -> String {
    format!("tenant: {}", matched.subdomain)
}

// For handlers serving both exact and wildcard routes:
async fn handler(matched: Option<MatchedHost>) -> String {
    match matched {
        Some(h) => format!("tenant: {}", h.subdomain),
        None => "no tenant".to_string(),
    }
}
```

Host resolution checks headers in order: `Forwarded` (RFC 7239 `host=` directive), `X-Forwarded-Host`, then `Host`. Values are lowercased and port-stripped.

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
