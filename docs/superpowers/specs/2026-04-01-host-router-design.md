# Host-Based Router Design

**Date:** 2026-04-01
**Module:** `server::HostRouter`

## Overview

A host-based routing primitive for modo that dispatches requests to different axum `Router`s based on the `Host` header. Designed for multi-app deployments where different hosts serve different applications:

- `acme.com` -> landing page
- `app.acme.com` -> admin panel
- `*.acme.com` -> tenant-facing portals

## Design Decisions

1. **Independent of tenant system** — pure routing primitive, no tenant coupling
2. **Configurable fallback** — optional fallback router; 404 if none
3. **Exact + single-level wildcard matching** — both O(1) via `HashMap`
4. **`MatchedHost` extractor** — injected into extensions on wildcard match
5. **Single `.host()` method** — detects `*` prefix automatically
6. **Lives in `server` module** — `modo::server::HostRouter`
7. **`Into<Router>` integration** — `server::http()` accepts `impl Into<Router>`
8. **No built-in `.layer()`** — use external tower `ServiceBuilder`

## API

```rust
use modo::server::{self, HostRouter, MatchedHost};

let host_router = HostRouter::new()
    .host("acme.com", landing_router)
    .host("app.acme.com", admin_router)
    .host("*.acme.com", tenant_router)
    .fallback(not_configured_router);

let server = server::http(host_router, &config.server).await?;
modo::run!(server).await
```

Handlers on wildcard-matched routers can extract the matched subdomain:

```rust
async fn handler(matched: MatchedHost) -> impl IntoResponse {
    format!("tenant: {}", matched.subdomain)
}

// Or optionally, when the handler serves both exact and wildcard routes:
async fn handler(matched: Option<MatchedHost>) -> impl IntoResponse {
    match matched {
        Some(h) => format!("tenant: {}", h.subdomain),
        None => "no tenant".to_string(),
    }
}
```

## Core Types

### `HostRouter`

```rust
pub struct HostRouter {
    inner: Arc<HostRouterInner>,
}

struct HostRouterInner {
    exact: HashMap<String, Router>,
    wildcard: HashMap<String, Router>,
    fallback: Option<Router>,
}
```

Uses the `Arc<Inner>` pattern — `Inner` is private, `Clone` is cheap.

Builder methods consume `self` and return `Self` (move semantics, like axum's `Router`):

```rust
impl HostRouter {
    pub fn new() -> Self;
    pub fn host(self, pattern: &str, router: Router) -> Self;
    pub fn fallback(self, router: Router) -> Self;
}
```

### `MatchedHost`

```rust
#[derive(Debug, Clone)]
pub struct MatchedHost {
    pub subdomain: String,
    pub pattern: String,
}
```

Implements `FromRequestParts` (returns `Error` if not present) and `OptionalFromRequestParts` (returns `None`).

Injected into request extensions on wildcard match only.

## Host Resolution

Checks headers in this order for reverse proxy compatibility (Caddy, Traefik, cloud PaaS):

1. `Forwarded` header (RFC 7239) — parse `host=` directive from the first element. Format: `Forwarded: for=...; host=example.com; proto=https`. Extract the value of `host=`, ignoring other directives.
2. `X-Forwarded-Host` — take the first value (leftmost proxy)
3. `Host` header

After extraction: strip port, lowercase.

Returns `Error::bad_request("missing or invalid Host header")` if no host can be resolved from any of the three sources.

## Matching Algorithm

```
1. Parse host from request (forwarded headers -> Host header)
2. Strip port, lowercase
3. Exact match: exact.get(host) -> O(1)
4. Wildcard match:
   a. Find first dot in host
   b. If no dot -> skip to fallback
   c. subdomain = host[..dot], suffix = host[dot+1..]
   d. wildcard.get(suffix) -> O(1)
   e. If found -> inject MatchedHost { subdomain, pattern: "*.{suffix}" }
5. Fallback router, or 404 Not Found
```

Priority: exact always wins over wildcard. `app.acme.com` registered as exact takes precedence over `*.acme.com`.

Single-level only: `a.b.acme.com` does not match `*.acme.com` because the first-dot split produces `subdomain = "a"`, `suffix = "b.acme.com"`, which won't be in the wildcard map (only `"acme.com"` is registered).

## Construction-Time Validation

Panics (same convention as axum's `Router` for duplicate routes):

- **Invalid wildcard**: suffix after `*.` must contain at least one dot. Rejects `*.com`, `*`, `*.`
- **Duplicate exact host**: same host string registered twice
- **Duplicate wildcard suffix**: same suffix registered twice (e.g., two `*.acme.com`)

All patterns are lowercased and port-stripped at construction time.

## Integration with `server::http()`

Minimal change to `server/http.rs`:

```rust
// Before:
pub async fn http(router: axum::Router, config: &Config) -> Result<HttpServer>

// After:
pub async fn http(router: impl Into<axum::Router>, config: &Config) -> Result<HttpServer> {
    let router = router.into();
    // ... rest unchanged
}
```

`HostRouter` implements `From<HostRouter> for axum::Router`:

```rust
impl From<HostRouter> for axum::Router {
    fn from(host_router: HostRouter) -> axum::Router {
        axum::Router::new().fallback_service(host_router.inner)
    }
}
```

An empty `Router` with a `fallback_service` means every request hits the host dispatcher. `HostRouterInner` implements `Service<Request<Body>>` with `Error = Infallible` — all errors become HTTP responses.

`axum::Router` already implements `Into<axum::Router>`, so existing code is unaffected.

## Error Handling

| Condition | Response |
|---|---|
| Missing/invalid Host header (no forwarded headers either) | 400 Bad Request |
| No matching host and no fallback | 404 Not Found |
| Sub-router returns error | Passthrough (sub-router's own error handling) |

All errors use `modo::Error` — no custom error types. Errors become responses via `Error::into_response()`.

## Performance

~100-200ns overhead per request:

- Host resolution: ~50-100ns (header lookups, port strip, lowercase)
- Matching: ~20-50ns (one or two `HashMap::get()` calls)
- `MatchedHost` injection: ~10-20ns (wildcard only)
- Router clone: ~5ns (atomic increment on `Arc`)

Memory: two `HashMap`s with a handful of entries. `MatchedHost` is ~80 bytes, allocated on wildcard matches only.

## Module Structure

```
src/server/
    mod.rs          <- adds `mod host_router;` + re-exports
    config.rs       <- existing, unchanged
    http.rs         <- signature change: impl Into<Router>
    host_router.rs  <- new: HostRouter, HostRouterInner, MatchedHost, From impl
```

No feature flag — always available. No new dependencies beyond what `server` already uses (axum, tower, http).

Re-exports from `server/mod.rs`:
```rust
pub use host_router::{HostRouter, MatchedHost};
```

## Testing

Unit tests in `host_router.rs`:

**Routing:**
- Exact match routes to correct router
- Wildcard match routes to correct router and injects `MatchedHost`
- Exact takes priority over wildcard
- Bare domain doesn't match wildcard (`acme.com` vs `*.acme.com`)
- Multi-level subdomain doesn't match (`a.b.acme.com` vs `*.acme.com`)
- Fallback used when no match
- 404 when no match and no fallback
- 400 on missing Host header

**Host resolution:**
- `Forwarded: host=...` takes priority over `X-Forwarded-Host`
- `X-Forwarded-Host` takes priority over `Host`
- Port stripping works
- Case-insensitive matching

**Construction panics (`#[should_panic]`):**
- Duplicate exact hosts
- Duplicate wildcard suffixes
- Invalid wildcards: `*.com`, `*`, `*.`
