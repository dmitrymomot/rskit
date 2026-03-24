# modo::middleware

HTTP middleware for the modo web framework.

A collection of Tower-compatible middleware layers covering common cross-cutting concerns. All items are re-exported from `modo::middleware`.

## Key Types

| Item                             | Kind   | Purpose                                                  |
| -------------------------------- | ------ | -------------------------------------------------------- |
| `compression`                    | fn     | Response compression (gzip, deflate, brotli, zstd)       |
| `request_id`                     | fn     | Set / propagate `x-request-id` (ULID-based)              |
| `catch_panic`                    | fn     | Convert handler panics into 500 responses                |
| `tracing`                        | fn     | HTTP request/response lifecycle tracing spans            |
| `cors` / `cors_with`             | fn     | CORS headers — static or dynamic origins                 |
| `subdomains` / `urls`            | fn     | CORS origin predicates                                   |
| `CorsConfig`                     | struct | CORS configuration                                       |
| `csrf`                           | fn     | Double-submit signed-cookie CSRF protection              |
| `CsrfConfig`                     | struct | CSRF configuration                                       |
| `CsrfToken`                      | struct | CSRF token in request/response extensions                |
| `error_handler`                  | fn     | Centralised error-response rendering                     |
| `security_headers`               | fn     | Add security headers to every response                   |
| `SecurityHeadersConfig`          | struct | Security headers configuration                           |
| `rate_limit` / `rate_limit_with` | fn     | Token-bucket rate limiting                               |
| `RateLimitConfig`                | struct | Rate-limit configuration                                 |
| `RateLimitLayer`                 | struct | Tower layer produced by `rate_limit` / `rate_limit_with` |
| `KeyExtractor`                   | trait  | Custom rate-limit key extraction                         |
| `PeerIpKeyExtractor`             | struct | Rate-limit key from peer IP                              |
| `GlobalKeyExtractor`             | struct | Single shared rate-limit bucket                          |

## Usage

### Compression, request IDs, and tracing

```rust
use axum::Router;
use axum::routing::get;
use modo::middleware::{catch_panic, compression, request_id, tracing};

let app = Router::new()
    .route("/", get(|| async { "hello" }))
    .layer(compression())
    .layer(request_id())
    .layer(catch_panic())
    .layer(tracing());
```

### CORS

```rust
use modo::middleware::{CorsConfig, cors, cors_with, subdomains};

// Static allow-list
let config = CorsConfig { origins: vec!["https://example.com".to_string()], ..Default::default() };
let layer = cors(&config);

// Dynamic: any subdomain of example.com
let layer = cors_with(&config, subdomains("example.com"));
```

### CSRF protection

```rust
use modo::middleware::{csrf, CsrfConfig};
use modo::cookie::Key;

let config = CsrfConfig::default();
let key = Key::generate();
let layer = csrf(&config, &key);
```

Handlers receive the token via `CsrfToken` in request extensions. Unsafe methods (POST, PUT, DELETE, PATCH) must send the token in the `X-CSRF-Token` header.

### Security headers

```rust
use modo::middleware::{security_headers, SecurityHeadersConfig};

let config = SecurityHeadersConfig {
    hsts_max_age: Some(31536000),
    content_security_policy: Some("default-src 'self'".to_string()),
    ..Default::default()
};
let layer = security_headers(&config);
```

### Error handler

```rust
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

```rust
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

### Custom key extractor

```rust
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
