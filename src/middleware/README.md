# modo::middleware

Universal HTTP middleware for the modo web framework.

A collection of Tower-compatible middleware layers covering common cross-cutting concerns. Always available — no feature flag required. All items are re-exported from `modo::middleware`.

See also the virtual [`modo::middlewares`](../middlewares.rs) flat index, which re-exports both these universal layers and per-domain layers (`session`, `tenant`, `auth`, `flash`, `ip`, `tier`, `geolocation`, `template`) under one namespace for `.layer(...)` call sites.

## Key Types

### Security

| Item                    | Kind   | Purpose                                     |
| ----------------------- | ------ | ------------------------------------------- |
| `cors` / `cors_with`    | fn     | CORS headers — static or dynamic origins    |
| `subdomains` / `urls`   | fn     | CORS origin predicates                      |
| `CorsConfig`            | struct | CORS configuration                          |
| `csrf`                  | fn     | Double-submit signed-cookie CSRF protection |
| `CsrfConfig`            | struct | CSRF configuration                          |
| `CsrfToken`             | struct | CSRF token in request/response extensions   |
| `security_headers`      | fn     | Add security headers to every response      |
| `SecurityHeadersConfig` | struct | Security headers configuration              |

### Performance & resource control

| Item                             | Kind   | Purpose                                                  |
| -------------------------------- | ------ | -------------------------------------------------------- |
| `compression`                    | fn     | Response compression (gzip, deflate, brotli, zstd)       |
| `rate_limit` / `rate_limit_with` | fn     | Token-bucket rate limiting                               |
| `RateLimitConfig`                | struct | Rate-limit configuration                                 |
| `RateLimitLayer`                 | struct | Tower layer produced by `rate_limit` / `rate_limit_with` |
| `KeyExtractor`                   | trait  | Custom rate-limit key extraction                         |
| `PeerIpKeyExtractor`             | struct | Rate-limit key from peer IP                              |
| `GlobalKeyExtractor`             | struct | Single shared rate-limit bucket                          |

### Observability

| Item         | Kind | Purpose                                       |
| ------------ | ---- | --------------------------------------------- |
| `request_id` | fn   | Set / propagate `x-request-id` (ULID-based)   |
| `tracing`    | fn   | HTTP request/response lifecycle tracing spans |

### Control flow

| Item            | Kind | Purpose                                   |
| --------------- | ---- | ----------------------------------------- |
| `catch_panic`   | fn   | Convert handler panics into 500 responses |
| `error_handler` | fn   | Centralised error-response rendering      |

### Header sanitization

| Item             | Kind   | Purpose                                                       |
| ---------------- | ------ | ------------------------------------------------------------- |
| `UserAgentLayer` | struct | Bound and clean the inbound `User-Agent` header for consumers |

## Usage

### Layer composition

axum applies `.layer(...)` in reverse declaration order — the last layer added wraps everything before it and runs first on the inbound path. The idiomatic stack for this module is, from outer to inner:

1. `tracing()` — outermost, so every request is observed inside `http_request`.
2. `catch_panic()` — converts panics to 500s that `error_handler` can still re-render.
3. `request_id()` — sets `x-request-id` on every response, including errors.
4. `compression()` — close to the handler so compressed bytes flow through the fewest layers.
5. `error_handler(handler)` — innermost cross-cutting layer; rewrites any response carrying a `modo::Error` in its extensions.

```rust,ignore
use axum::{Router, routing::get};
use axum::response::IntoResponse;
use http::request::Parts;
use modo::middleware::{catch_panic, compression, error_handler, request_id, tracing};

async fn render_error(err: modo::Error, _parts: Parts) -> axum::response::Response {
    (err.status(), err.message().to_string()).into_response()
}

let app: Router = Router::new()
    .route("/", get(|| async { "hello" }))
    .layer(error_handler(render_error))  // innermost
    .layer(compression())
    .layer(request_id())
    .layer(catch_panic())
    .layer(tracing());                    // outermost
```

### `.layer` vs `.route_layer`

Use `Router::layer(...)` for middleware that should run for every request the router sees, including 404s synthesized by axum. Use `Router::route_layer(...)` when the middleware must only see requests that matched a real route — for example, authorization guards that otherwise would rewrite a 404 into a 401. All middleware in this module is designed for `.layer(...)`; domain guards from `auth`, `tier`, etc. typically want `.route_layer`.

The `Router::layer` bounds require the wrapped `L` and `L::Service` to be `+ Sync`, with errors convertible `Into<Infallible>`. All middleware constructors in this module satisfy those bounds.

### CORS

```rust,ignore
use modo::middleware::{CorsConfig, cors, cors_with, subdomains};

// Static allow-list
let config = CorsConfig { origins: vec!["https://example.com".to_string()], ..Default::default() };
let layer = cors(&config);

// Dynamic: any subdomain of example.com
let layer = cors_with(&config, subdomains("example.com"));
```

### CSRF protection

```rust,ignore
use modo::middleware::{csrf, CsrfConfig};
use modo::cookie::Key;

let config = CsrfConfig::default();
let key = Key::generate();
let layer = csrf(&config, &key);
```

Handlers receive the token via `CsrfToken` in request extensions. Unsafe methods (POST, PUT, DELETE, PATCH) must send the token in the `X-CSRF-Token` header.

### Security headers

```rust,ignore
use modo::middleware::{security_headers, SecurityHeadersConfig};

let config = SecurityHeadersConfig {
    hsts_max_age: Some(31536000),
    content_security_policy: Some("default-src 'self'".to_string()),
    ..Default::default()
};
let layer = security_headers(&config).unwrap();
```

### Error handler

```rust,ignore
use axum::response::IntoResponse;
use http::request::Parts;
use modo::middleware::error_handler;

async fn render_error(err: modo::Error, _parts: Parts) -> axum::response::Response {
    (err.status(), err.message().to_string()).into_response()
}

let layer = error_handler(render_error);
```

The handler fires whenever a `modo::Error` is present in response extensions — catches errors from `catch_panic`, `csrf`, `rate_limit`, and handler errors.

### Rate limiting

```rust,ignore
use tokio_util::sync::CancellationToken;
use modo::middleware::{rate_limit, rate_limit_with, GlobalKeyExtractor, RateLimitConfig};

let cancel = CancellationToken::new();
let config = RateLimitConfig { per_second: 10, burst_size: 30, ..Default::default() };

// Per-IP (requires into_make_service_with_connect_info)
let layer = rate_limit(&config, cancel.clone());

// Global shared bucket
let layer = rate_limit_with(&config, GlobalKeyExtractor, cancel.clone());
```

When `use_headers` is `true` (default), allowed responses carry `x-ratelimit-limit`, `x-ratelimit-remaining`, and `x-ratelimit-reset`; rejected responses carry `retry-after`.

### User-Agent sanitization

`UserAgentLayer` rewrites the inbound `User-Agent` header in place before any downstream layer or handler reads it. The sanitizer truncates the value to a configurable byte cap (default 512) on a UTF-8 char boundary, drops ASCII control characters, collapses runs of ASCII whitespace into a single space, and trims. If the result is empty the header is removed entirely so consumers see the same "missing" state they handle today.

```rust,ignore
use axum::{Router, routing::get};
use modo::middleware::UserAgentLayer;

let app: Router = Router::new()
    .route("/", get(|| async { "ok" }))
    .layer(UserAgentLayer::new());            // default 512-byte cap

// Or with a custom cap:
let app: Router = Router::new()
    .route("/", get(|| async { "ok" }))
    .layer(UserAgentLayer::with_max_length(256));
```

Because the layer mutates the request header itself, every downstream consumer — `ClientInfo`, the cookie session middleware, audit logging, fingerprint hashing — observes the sanitized value with no further plumbing. Install it **before** any layer or handler that reads `User-Agent`; in axum's outer-runs-first ordering that means adding it after (i.e. wrapping) the consumer.

### Custom key extractor

```rust,ignore
use http::Request;
use modo::middleware::{KeyExtractor, rate_limit_with, RateLimitConfig};
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
struct ApiKeyExtractor;

impl KeyExtractor for ApiKeyExtractor {
    fn extract<B>(&self, req: &Request<B>) -> Option<String> {
        req.headers()
            .get("x-api-key")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string())
    }
}

let layer = rate_limit_with(&RateLimitConfig::default(), ApiKeyExtractor, CancellationToken::new());
```

## Configuration

All `*Config` types implement `serde::Deserialize` with `#[serde(default)]` and load cleanly from YAML via `modo::config`. The default values are:

- `RateLimitConfig`: `per_second=1`, `burst_size=10`, `use_headers=true`, `cleanup_interval_secs=60`, `max_keys=10000`
- `CorsConfig`: allow any origin, methods GET/POST/PUT/DELETE/PATCH, `max_age_secs=86400`
- `CsrfConfig`: cookie `_csrf`, header `X-CSRF-Token`, `ttl_secs=21600`, exempt GET/HEAD/OPTIONS
- `SecurityHeadersConfig`: `x_content_type_options=true`, `x_frame_options=DENY`, `referrer_policy=strict-origin-when-cross-origin`
