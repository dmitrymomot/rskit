# Tenant Resolution Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Multi-tenant resolution middleware that extracts tenant identity from HTTP requests, resolves to app-defined types via async DB lookup, and enforces tenant presence at the middleware level.

**Architecture:** Strategy pattern (7 strategies extract `TenantId` from requests) + Resolver trait (app implements DB lookup) + Tower middleware (enforces resolution, inserts into extensions) + Extractor (`Tenant<T>` pulls from extensions). No feature flag — always available.

**Tech Stack:** axum 0.8, tower 0.5, tower-http 0.6, tracing 0.1

**Spec:** `docs/superpowers/specs/2026-03-22-modo-v2-tenant-design.md`

---

## File Structure

```
src/tenant/
    mod.rs          — mod imports + re-exports
    id.rs           — TenantId enum, Display, Debug, as_str()
    traits.rs       — HasTenantId, TenantResolver, TenantStrategy traits
    strategy.rs     — all built-in strategy structs + constructor functions
    middleware.rs   — TenantLayer, TenantService, tenant::middleware() constructor
    extractor.rs    — Tenant<T> struct + Deref + FromRequestParts impl

Modified:
    src/lib.rs      — add `pub mod tenant` + re-exports
    src/middleware/tracing.rs — add tenant_id Empty field to request span
```

---

### Task 1: Module scaffolding

**Files:**
- Create: `src/tenant/mod.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Create tenant module directory and mod.rs**

```rust
// src/tenant/mod.rs
mod extractor;
mod id;
mod middleware;
mod strategy;
mod traits;

pub use extractor::Tenant;
pub use id::TenantId;
pub use middleware::middleware;
pub use strategy::{
    api_key_header, domain, header, path_param, path_prefix, subdomain, subdomain_or_domain,
    ApiKeyHeaderStrategy, DomainStrategy, HeaderStrategy, PathParamStrategy, PathPrefixStrategy,
    SubdomainOrDomainStrategy, SubdomainStrategy,
};
pub use traits::{HasTenantId, TenantResolver, TenantStrategy};
```

- [ ] **Step 2: Add pub mod tenant to lib.rs**

Add `pub mod tenant;` to `src/lib.rs` alongside other modules. Add re-exports:

```rust
pub use tenant::{HasTenantId, Tenant, TenantId, TenantResolver, TenantStrategy};
```

- [ ] **Step 3: Create empty placeholder files**

Create empty files with just enough to compile:
- `src/tenant/id.rs` — `pub enum TenantId { Slug(String), Domain(String), Id(String), ApiKey(String) }`
- `src/tenant/traits.rs` — empty traits
- `src/tenant/strategy.rs` — stub constructor functions
- `src/tenant/middleware.rs` — stub `middleware()` function
- `src/tenant/extractor.rs` — stub `Tenant<T>` struct

- [ ] **Step 4: Verify it compiles**

Run: `cargo check`
Expected: compiles with no errors (stubs may have warnings)

- [ ] **Step 5: Commit**

```bash
git add src/tenant/ src/lib.rs
git commit -m "feat(tenant): scaffold tenant module with stubs"
```

---

### Task 2: TenantId enum

**Files:**
- Create: `src/tenant/id.rs`

- [ ] **Step 1: Write tests for TenantId**

```rust
// src/tenant/id.rs — at bottom
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_slug() {
        let id = TenantId::Slug("acme".into());
        assert_eq!(id.to_string(), "slug:acme");
    }

    #[test]
    fn display_domain() {
        let id = TenantId::Domain("acme.com".into());
        assert_eq!(id.to_string(), "domain:acme.com");
    }

    #[test]
    fn display_id() {
        let id = TenantId::Id("abc123".into());
        assert_eq!(id.to_string(), "id:abc123");
    }

    #[test]
    fn display_api_key_redacted() {
        let id = TenantId::ApiKey("sk_live_secret".into());
        assert_eq!(id.to_string(), "apikey:[REDACTED]");
    }

    #[test]
    fn debug_api_key_redacted() {
        let id = TenantId::ApiKey("sk_live_secret".into());
        let debug = format!("{:?}", id);
        assert!(!debug.contains("sk_live_secret"));
        assert!(debug.contains("REDACTED"));
    }

    #[test]
    fn as_str_returns_inner_value() {
        assert_eq!(TenantId::Slug("acme".into()).as_str(), "acme");
        assert_eq!(TenantId::Domain("acme.com".into()).as_str(), "acme.com");
        assert_eq!(TenantId::Id("abc123".into()).as_str(), "abc123");
        assert_eq!(TenantId::ApiKey("sk_live".into()).as_str(), "sk_live");
    }

    #[test]
    fn equality() {
        let a = TenantId::Slug("acme".into());
        let b = TenantId::Slug("acme".into());
        assert_eq!(a, b);

        let c = TenantId::Domain("acme".into());
        assert_ne!(a, c);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib -- tenant::id::tests -v`
Expected: FAIL — Display/Debug not implemented

- [ ] **Step 3: Implement TenantId**

```rust
// src/tenant/id.rs
use std::fmt;

/// Identifier extracted from an HTTP request by a tenant strategy.
#[derive(Clone, PartialEq, Eq)]
pub enum TenantId {
    /// From subdomain, path_prefix, path_param strategies.
    Slug(String),
    /// From domain(), combined strategy's domain branch.
    Domain(String),
    /// From header() — generic identifier.
    Id(String),
    /// From api_key_header() — raw API key.
    ApiKey(String),
}

impl TenantId {
    /// Returns the inner string regardless of variant.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Slug(s) | Self::Domain(s) | Self::Id(s) | Self::ApiKey(s) => s,
        }
    }
}

impl fmt::Display for TenantId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Slug(s) => write!(f, "slug:{s}"),
            Self::Domain(s) => write!(f, "domain:{s}"),
            Self::Id(s) => write!(f, "id:{s}"),
            Self::ApiKey(_) => write!(f, "apikey:[REDACTED]"),
        }
    }
}

impl fmt::Debug for TenantId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Slug(s) => f.debug_tuple("Slug").field(s).finish(),
            Self::Domain(s) => f.debug_tuple("Domain").field(s).finish(),
            Self::Id(s) => f.debug_tuple("Id").field(s).finish(),
            Self::ApiKey(_) => f.debug_tuple("ApiKey").field(&"[REDACTED]").finish(),
        }
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib -- tenant::id::tests -v`
Expected: all pass

- [ ] **Step 5: Commit**

```bash
git add src/tenant/id.rs
git commit -m "feat(tenant): implement TenantId enum with Display, Debug, as_str"
```

---

### Task 3: Traits

**Files:**
- Create: `src/tenant/traits.rs`

- [ ] **Step 1: Implement all three traits**

No tests needed for trait definitions — they'll be tested through strategy/middleware/extractor tests.

```rust
// src/tenant/traits.rs
use std::future::Future;

use crate::Result;

use super::TenantId;

/// Resolved tenant type must implement this to provide identity for tracing.
pub trait HasTenantId {
    /// Returns the tenant's unique identifier for tracing spans.
    fn tenant_id(&self) -> &str;
}

/// Extracts a `TenantId` from an HTTP request.
///
/// Each strategy inspects different parts of the request (Host header,
/// path, custom header) and produces the appropriate `TenantId` variant.
pub trait TenantStrategy: Send + Sync + 'static {
    /// Extract tenant identifier from request parts.
    ///
    /// Takes `&mut Parts` to allow URI rewriting (used by `PathPrefixStrategy`).
    fn extract(&self, parts: &mut http::request::Parts) -> Result<TenantId>;
}

/// Resolves a `TenantId` to an app-defined tenant type.
///
/// App implements this trait with their DB lookup logic.
/// Uses RPITIT — not object-safe; resolver is a concrete type.
pub trait TenantResolver: Send + Sync + 'static {
    /// The resolved tenant type.
    type Tenant: HasTenantId + Send + Sync + Clone + 'static;

    /// Look up a tenant by the extracted identifier.
    fn resolve(&self, id: &TenantId) -> impl Future<Output = Result<Self::Tenant>> + Send;
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check`
Expected: compiles

- [ ] **Step 3: Commit**

```bash
git add src/tenant/traits.rs
git commit -m "feat(tenant): define HasTenantId, TenantStrategy, TenantResolver traits"
```

---

### Task 4: Host-based strategies

**Files:**
- Create: `src/tenant/strategy.rs`

These share a `host_from_parts()` helper that extracts and strips port from the Host header.

- [ ] **Step 1: Write tests for host helper and SubdomainStrategy**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn make_parts(host: Option<&str>, uri: &str) -> http::request::Parts {
        let mut builder = http::Request::builder().uri(uri);
        if let Some(h) = host {
            builder = builder.header("host", h);
        }
        let (parts, _) = builder.body(()).unwrap().into_parts();
        parts
    }

    // --- host helper ---

    #[test]
    fn host_strips_port() {
        let mut parts = make_parts(Some("acme.com:8080"), "/");
        assert_eq!(host_from_parts(&mut parts).unwrap(), "acme.com");
    }

    #[test]
    fn host_missing_returns_error() {
        let mut parts = make_parts(None, "/");
        assert!(host_from_parts(&mut parts).is_err());
    }

    // --- SubdomainStrategy ---

    #[test]
    fn subdomain_valid() {
        let s = SubdomainStrategy::new("acme.com");
        let mut parts = make_parts(Some("app.acme.com"), "/");
        let id = s.extract(&mut parts).unwrap();
        assert_eq!(id, TenantId::Slug("app".into()));
    }

    #[test]
    fn subdomain_bare_base_domain_error() {
        let s = SubdomainStrategy::new("acme.com");
        let mut parts = make_parts(Some("acme.com"), "/");
        assert!(s.extract(&mut parts).is_err());
    }

    #[test]
    fn subdomain_multi_level_error() {
        let s = SubdomainStrategy::new("acme.com");
        let mut parts = make_parts(Some("test.app.acme.com"), "/");
        assert!(s.extract(&mut parts).is_err());
    }

    #[test]
    fn subdomain_multi_level_base_domain() {
        let s = SubdomainStrategy::new("app.acme.com");
        let mut parts = make_parts(Some("test.app.acme.com"), "/");
        let id = s.extract(&mut parts).unwrap();
        assert_eq!(id, TenantId::Slug("test".into()));
    }

    #[test]
    fn subdomain_port_stripped() {
        let s = SubdomainStrategy::new("acme.com");
        let mut parts = make_parts(Some("app.acme.com:3000"), "/");
        let id = s.extract(&mut parts).unwrap();
        assert_eq!(id, TenantId::Slug("app".into()));
    }

    #[test]
    fn subdomain_missing_host() {
        let s = SubdomainStrategy::new("acme.com");
        let mut parts = make_parts(None, "/");
        assert!(s.extract(&mut parts).is_err());
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib -- tenant::strategy::tests -v`
Expected: FAIL — structs not defined

- [ ] **Step 3: Implement host helper and SubdomainStrategy**

```rust
// src/tenant/strategy.rs
use crate::{Error, Result};

use super::traits::TenantStrategy;
use super::TenantId;

/// Extract Host header value, strip port if present.
fn host_from_parts(parts: &http::request::Parts) -> Result<String> {
    let host = parts
        .headers
        .get(http::header::HOST)
        .ok_or_else(|| Error::bad_request("missing Host header"))?
        .to_str()
        .map_err(|_| Error::bad_request("invalid Host header"))?;

    // Strip port
    let host = match host.rfind(':') {
        Some(pos) if host[pos + 1..].bytes().all(|b| b.is_ascii_digit()) => &host[..pos],
        _ => host,
    };

    Ok(host.to_lowercase())
}

/// Extracts tenant slug from subdomain. Only one level allowed.
pub struct SubdomainStrategy {
    base_domain: String,
}

impl SubdomainStrategy {
    fn new(base_domain: &str) -> Self {
        Self {
            base_domain: base_domain.to_lowercase(),
        }
    }
}

impl TenantStrategy for SubdomainStrategy {
    fn extract(&self, parts: &mut http::request::Parts) -> Result<TenantId> {
        let host = host_from_parts(parts)?;
        let suffix = format!(".{}", self.base_domain);

        if !host.ends_with(&suffix) {
            return Err(Error::bad_request("host is not a subdomain of base domain"));
        }

        let subdomain = &host[..host.len() - suffix.len()];

        if subdomain.is_empty() {
            return Err(Error::bad_request("no subdomain in host"));
        }

        // Only one level allowed
        if subdomain.contains('.') {
            return Err(Error::bad_request("multi-level subdomains not allowed"));
        }

        Ok(TenantId::Slug(subdomain.to_string()))
    }
}

/// Constructor function.
pub fn subdomain(base_domain: &str) -> SubdomainStrategy {
    SubdomainStrategy::new(base_domain)
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib -- tenant::strategy::tests -v`
Expected: all pass

- [ ] **Step 5: Add DomainStrategy tests and implementation**

Tests:
```rust
    // --- DomainStrategy ---

    #[test]
    fn domain_valid() {
        let s = DomainStrategy;
        let mut parts = make_parts(Some("acme.com"), "/");
        let id = s.extract(&mut parts).unwrap();
        assert_eq!(id, TenantId::Domain("acme.com".into()));
    }

    #[test]
    fn domain_strips_port() {
        let s = DomainStrategy;
        let mut parts = make_parts(Some("acme.com:8080"), "/");
        let id = s.extract(&mut parts).unwrap();
        assert_eq!(id, TenantId::Domain("acme.com".into()));
    }

    #[test]
    fn domain_missing_host() {
        let s = DomainStrategy;
        let mut parts = make_parts(None, "/");
        assert!(s.extract(&mut parts).is_err());
    }
```

Implementation:
```rust
/// Extracts full Host header as tenant domain.
pub struct DomainStrategy;

impl TenantStrategy for DomainStrategy {
    fn extract(&self, parts: &mut http::request::Parts) -> Result<TenantId> {
        let host = host_from_parts(parts)?;
        Ok(TenantId::Domain(host))
    }
}

pub fn domain() -> DomainStrategy {
    DomainStrategy
}
```

- [ ] **Step 6: Run tests**

Run: `cargo test --lib -- tenant::strategy::tests -v`
Expected: all pass

- [ ] **Step 7: Add SubdomainOrDomainStrategy tests and implementation**

Tests:
```rust
    // --- SubdomainOrDomainStrategy ---

    #[test]
    fn subdomain_or_domain_subdomain_branch() {
        let s = SubdomainOrDomainStrategy::new("acme.com");
        let mut parts = make_parts(Some("app.acme.com"), "/");
        let id = s.extract(&mut parts).unwrap();
        assert_eq!(id, TenantId::Slug("app".into()));
    }

    #[test]
    fn subdomain_or_domain_custom_domain_branch() {
        let s = SubdomainOrDomainStrategy::new("acme.com");
        let mut parts = make_parts(Some("custom.com"), "/");
        let id = s.extract(&mut parts).unwrap();
        assert_eq!(id, TenantId::Domain("custom.com".into()));
    }

    #[test]
    fn subdomain_or_domain_base_domain_error() {
        let s = SubdomainOrDomainStrategy::new("acme.com");
        let mut parts = make_parts(Some("acme.com"), "/");
        assert!(s.extract(&mut parts).is_err());
    }

    #[test]
    fn subdomain_or_domain_multi_level_error() {
        let s = SubdomainOrDomainStrategy::new("acme.com");
        let mut parts = make_parts(Some("test.app.acme.com"), "/");
        assert!(s.extract(&mut parts).is_err());
    }

    #[test]
    fn subdomain_or_domain_missing_host() {
        let s = SubdomainOrDomainStrategy::new("acme.com");
        let mut parts = make_parts(None, "/");
        assert!(s.extract(&mut parts).is_err());
    }
```

Implementation:
```rust
/// Checks if Host is a subdomain of base domain; if not, treats as custom domain.
///
/// - Single-level subdomain of base → `Slug`
/// - Unrelated host → `Domain` (custom domain)
/// - Base domain exactly → Error (not valid for tenant routes)
/// - Multi-level subdomain → Error
pub struct SubdomainOrDomainStrategy {
    base_domain: String,
}

impl SubdomainOrDomainStrategy {
    fn new(base_domain: &str) -> Self {
        Self {
            base_domain: base_domain.to_lowercase(),
        }
    }
}

impl TenantStrategy for SubdomainOrDomainStrategy {
    fn extract(&self, parts: &mut http::request::Parts) -> Result<TenantId> {
        let host = host_from_parts(parts)?;
        let suffix = format!(".{}", self.base_domain);

        if host == self.base_domain {
            return Err(Error::bad_request(
                "base domain is not a valid tenant identifier",
            ));
        }

        if host.ends_with(&suffix) {
            // It's a subdomain — extract and validate single level
            let subdomain = &host[..host.len() - suffix.len()];

            if subdomain.is_empty() {
                return Err(Error::bad_request("no subdomain in host"));
            }

            if subdomain.contains('.') {
                return Err(Error::bad_request("multi-level subdomains not allowed"));
            }

            Ok(TenantId::Slug(subdomain.to_string()))
        } else {
            // Custom domain
            Ok(TenantId::Domain(host))
        }
    }
}

pub fn subdomain_or_domain(base_domain: &str) -> SubdomainOrDomainStrategy {
    SubdomainOrDomainStrategy::new(base_domain)
}
```

- [ ] **Step 8: Run all strategy tests**

Run: `cargo test --lib -- tenant::strategy::tests -v`
Expected: all pass

- [ ] **Step 9: Commit**

```bash
git add src/tenant/strategy.rs
git commit -m "feat(tenant): implement host-based strategies (subdomain, domain, combined)"
```

---

### Task 5: Header-based strategies

**Files:**
- Modify: `src/tenant/strategy.rs`

- [ ] **Step 1: Write tests for HeaderStrategy and ApiKeyHeaderStrategy**

```rust
    // --- HeaderStrategy ---

    #[test]
    fn header_valid() {
        let s = HeaderStrategy::new("x-tenant-id");
        let mut parts = make_parts(None, "/");
        parts.headers.insert("x-tenant-id", "abc123".parse().unwrap());
        let id = s.extract(&mut parts).unwrap();
        assert_eq!(id, TenantId::Id("abc123".into()));
    }

    #[test]
    fn header_missing_error() {
        let s = HeaderStrategy::new("x-tenant-id");
        let mut parts = make_parts(None, "/");
        assert!(s.extract(&mut parts).is_err());
    }

    // --- ApiKeyHeaderStrategy ---

    #[test]
    fn api_key_header_valid() {
        let s = ApiKeyHeaderStrategy::new("x-api-key");
        let mut parts = make_parts(None, "/");
        parts.headers.insert("x-api-key", "sk_live_secret".parse().unwrap());
        let id = s.extract(&mut parts).unwrap();
        assert_eq!(id, TenantId::ApiKey("sk_live_secret".into()));
    }

    #[test]
    fn api_key_header_missing_error() {
        let s = ApiKeyHeaderStrategy::new("x-api-key");
        let mut parts = make_parts(None, "/");
        assert!(s.extract(&mut parts).is_err());
    }

    #[test]
    fn header_non_utf8_error() {
        let s = HeaderStrategy::new("x-tenant-id");
        let mut parts = make_parts(None, "/");
        parts.headers.insert(
            "x-tenant-id",
            http::HeaderValue::from_bytes(&[0xff]).unwrap(),
        );
        assert!(s.extract(&mut parts).is_err());
    }

    #[test]
    fn api_key_header_non_utf8_error() {
        let s = ApiKeyHeaderStrategy::new("x-api-key");
        let mut parts = make_parts(None, "/");
        parts.headers.insert(
            "x-api-key",
            http::HeaderValue::from_bytes(&[0xff]).unwrap(),
        );
        assert!(s.extract(&mut parts).is_err());
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib -- tenant::strategy::tests::header -v`
Expected: FAIL

- [ ] **Step 3: Implement HeaderStrategy and ApiKeyHeaderStrategy**

```rust
/// Extracts tenant ID from a named request header.
pub struct HeaderStrategy {
    header_name: http::HeaderName,
}

impl HeaderStrategy {
    fn new(name: &str) -> Self {
        Self {
            header_name: http::HeaderName::from_bytes(name.as_bytes())
                .expect("invalid header name"),
        }
    }
}

impl TenantStrategy for HeaderStrategy {
    fn extract(&self, parts: &mut http::request::Parts) -> Result<TenantId> {
        let value = parts
            .headers
            .get(&self.header_name)
            .ok_or_else(|| {
                Error::bad_request(format!("missing {} header", self.header_name))
            })?
            .to_str()
            .map_err(|_| {
                Error::bad_request(format!("invalid {} header value", self.header_name))
            })?;

        Ok(TenantId::Id(value.to_string()))
    }
}

pub fn header(name: &str) -> HeaderStrategy {
    HeaderStrategy::new(name)
}

/// Extracts API key from a named request header.
pub struct ApiKeyHeaderStrategy {
    header_name: http::HeaderName,
}

impl ApiKeyHeaderStrategy {
    fn new(name: &str) -> Self {
        Self {
            header_name: http::HeaderName::from_bytes(name.as_bytes())
                .expect("invalid header name"),
        }
    }
}

impl TenantStrategy for ApiKeyHeaderStrategy {
    fn extract(&self, parts: &mut http::request::Parts) -> Result<TenantId> {
        let value = parts
            .headers
            .get(&self.header_name)
            .ok_or_else(|| {
                Error::bad_request(format!("missing {} header", self.header_name))
            })?
            .to_str()
            .map_err(|_| {
                Error::bad_request(format!("invalid {} header value", self.header_name))
            })?;

        Ok(TenantId::ApiKey(value.to_string()))
    }
}

pub fn api_key_header(name: &str) -> ApiKeyHeaderStrategy {
    ApiKeyHeaderStrategy::new(name)
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test --lib -- tenant::strategy::tests -v`
Expected: all pass

- [ ] **Step 5: Commit**

```bash
git add src/tenant/strategy.rs
git commit -m "feat(tenant): implement header and api_key_header strategies"
```

---

### Task 6: Path-based strategies

**Files:**
- Modify: `src/tenant/strategy.rs`

- [ ] **Step 1: Write tests for PathPrefixStrategy**

```rust
    // --- PathPrefixStrategy ---

    #[test]
    fn path_prefix_valid() {
        let s = PathPrefixStrategy::new("/t");
        let mut parts = make_parts(None, "/t/acme/dashboard");
        let id = s.extract(&mut parts).unwrap();
        assert_eq!(id, TenantId::Slug("acme".into()));
        assert_eq!(parts.uri.path(), "/dashboard");
    }

    #[test]
    fn path_prefix_only_slug() {
        let s = PathPrefixStrategy::new("/t");
        let mut parts = make_parts(None, "/t/acme");
        let id = s.extract(&mut parts).unwrap();
        assert_eq!(id, TenantId::Slug("acme".into()));
        assert_eq!(parts.uri.path(), "/");
    }

    #[test]
    fn path_prefix_wrong_prefix_error() {
        let s = PathPrefixStrategy::new("/t");
        let mut parts = make_parts(None, "/api/acme");
        assert!(s.extract(&mut parts).is_err());
    }

    #[test]
    fn path_prefix_no_segment_error() {
        let s = PathPrefixStrategy::new("/t");
        let mut parts = make_parts(None, "/t");
        assert!(s.extract(&mut parts).is_err());
    }

    #[test]
    fn path_prefix_no_segment_trailing_slash_error() {
        let s = PathPrefixStrategy::new("/t");
        let mut parts = make_parts(None, "/t/");
        assert!(s.extract(&mut parts).is_err());
    }

    #[test]
    fn path_prefix_preserves_query_string() {
        let s = PathPrefixStrategy::new("/t");
        let mut parts = make_parts(None, "/t/acme/dashboard?page=1");
        let id = s.extract(&mut parts).unwrap();
        assert_eq!(id, TenantId::Slug("acme".into()));
        assert_eq!(parts.uri.path_and_query().unwrap().as_str(), "/dashboard?page=1");
    }
```

- [ ] **Step 2: Write tests for PathParamStrategy**

`PathParamStrategy` reads axum path params from request extensions, which are only populated after route matching. Unit tests can't easily construct these internal types. Use integration-style tests with a real axum `Router` and `.route_layer()`:

```rust
    // --- PathParamStrategy --- (integration-style tests)

    #[tokio::test]
    async fn path_param_valid() {
        use axum::{routing::get, Router};
        use tower::ServiceExt;

        async fn handler(req: http::Request<axum::body::Body>) -> &'static str {
            let parts = req.extensions().get::<TenantId>();
            assert!(parts.is_some());
            "ok"
        }

        // Build a handler that manually calls PathParamStrategy
        async fn tenant_check(
            axum::extract::Path(params): axum::extract::Path<std::collections::HashMap<String, String>>,
            mut req: http::Request<axum::body::Body>,
        ) -> impl axum::response::IntoResponse {
            let tenant_slug = params.get("tenant").unwrap().clone();
            req.extensions_mut().insert(TenantId::Slug(tenant_slug));
            "ok"
        }

        // The real test: verify PathParamStrategy can read path params
        // when used via .route_layer(). Implementation must consult axum 0.8
        // source to find the correct extension type for raw path params.
        // Use `resolve-library-id` + `query-docs` MCP tools during implementation
        // to discover the exact API for accessing path params in middleware.
    }

    #[test]
    fn path_param_missing_returns_error() {
        // Without path params in extensions, extract should return 500
        let s = PathParamStrategy::new("tenant");
        let mut parts = make_parts(None, "/dashboard");
        let err = s.extract(&mut parts).unwrap_err();
        assert_eq!(err.status(), http::StatusCode::INTERNAL_SERVER_ERROR);
    }
```

**Implementation note:** The exact mechanism for reading axum path params in middleware depends on axum 0.8's internal types. During implementation, consult axum source code or docs (`resolve-library-id` + `query-docs` MCP tools) to find the correct extension type. The implementation may need to use `axum::extract::RawPathParams` or a similar type. Adjust the strategy and tests accordingly.

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test --lib -- tenant::strategy::tests::path -v`
Expected: FAIL

- [ ] **Step 4: Implement PathPrefixStrategy**

```rust
/// Extracts tenant slug from path prefix and rewrites URI.
pub struct PathPrefixStrategy {
    prefix: String,
}

impl PathPrefixStrategy {
    fn new(prefix: &str) -> Self {
        Self {
            prefix: prefix.to_string(),
        }
    }
}

impl TenantStrategy for PathPrefixStrategy {
    fn extract(&self, parts: &mut http::request::Parts) -> Result<TenantId> {
        let path = parts.uri.path();

        if !path.starts_with(&self.prefix) {
            return Err(Error::bad_request(format!(
                "path does not start with prefix '{}'",
                self.prefix
            )));
        }

        let after_prefix = &path[self.prefix.len()..];

        // Must have /slug after prefix
        let after_prefix = after_prefix
            .strip_prefix('/')
            .ok_or_else(|| Error::bad_request("no tenant segment after prefix"))?;

        if after_prefix.is_empty() {
            return Err(Error::bad_request("no tenant segment after prefix"));
        }

        // Split slug from remaining path
        let (slug, remaining) = match after_prefix.find('/') {
            Some(pos) => (&after_prefix[..pos], &after_prefix[pos..]),
            None => (after_prefix, "/"),
        };

        if slug.is_empty() {
            return Err(Error::bad_request("empty tenant slug in path"));
        }

        // Rewrite URI — preserve query string
        let new_path_and_query = match parts.uri.query() {
            Some(q) => format!("{remaining}?{q}"),
            None => remaining.to_string(),
        };
        let new_uri = http::Uri::builder()
            .path_and_query(new_path_and_query)
            .build()
            .map_err(|e| Error::internal(format!("failed to rewrite URI: {e}")))?;
        parts.uri = new_uri;

        Ok(TenantId::Slug(slug.to_string()))
    }
}

pub fn path_prefix(prefix: &str) -> PathPrefixStrategy {
    PathPrefixStrategy::new(prefix)
}
```

- [ ] **Step 5: Implement PathParamStrategy**

```rust
/// Reads tenant slug from a named axum path parameter.
///
/// Requires `.route_layer()` instead of `.layer()` — path params
/// are only available after axum route matching.
pub struct PathParamStrategy {
    param_name: String,
}

impl PathParamStrategy {
    fn new(name: &str) -> Self {
        Self {
            param_name: name.to_string(),
        }
    }
}

impl TenantStrategy for PathParamStrategy {
    fn extract(&self, parts: &mut http::request::Parts) -> Result<TenantId> {
        // axum stores matched path params in extensions after route matching.
        // Access via axum's internal RawPathParams type.
        let params = parts
            .extensions
            .get::<axum::extract::RawPathParams>()
            .ok_or_else(|| {
                Error::internal(
                    "path params not found — PathParamStrategy requires .route_layer()",
                )
            })?;

        for (key, value) in params.iter() {
            if key.as_ref() == self.param_name {
                let decoded = value.as_str().to_string();
                return Ok(TenantId::Slug(decoded));
            }
        }

        Err(Error::internal(format!(
            "path param '{}' not found in route",
            self.param_name
        )))
    }
}

pub fn path_param(name: &str) -> PathParamStrategy {
    PathParamStrategy::new(name)
}
```

**Implementation note:** `axum::extract::RawPathParams` is the internal type axum uses for path params. Check the exact public/private status in axum 0.8 and adjust — if it's not public, use `axum::extract::Path::<HashMap<String, String>>::from_request_parts` approach instead (call the axum extractor within the strategy). Consult axum docs with `resolve-library-id` + `query-docs` MCP tools during implementation.

- [ ] **Step 6: Run all strategy tests**

Run: `cargo test --lib -- tenant::strategy::tests -v`
Expected: all pass

- [ ] **Step 7: Commit**

```bash
git add src/tenant/strategy.rs
git commit -m "feat(tenant): implement path_prefix and path_param strategies"
```

---

### Task 7: Tenant extractor

**Files:**
- Create: `src/tenant/extractor.rs`

- [ ] **Step 1: Write tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[derive(Clone, Debug)]
    struct TestTenant {
        id: String,
        name: String,
    }

    impl HasTenantId for TestTenant {
        fn tenant_id(&self) -> &str {
            &self.id
        }
    }

    #[test]
    fn tenant_get() {
        let t = Tenant(Arc::new(TestTenant {
            id: "t1".into(),
            name: "Test".into(),
        }));
        assert_eq!(t.get().id, "t1");
        assert_eq!(t.get().name, "Test");
    }

    #[test]
    fn tenant_deref() {
        let t = Tenant(Arc::new(TestTenant {
            id: "t1".into(),
            name: "Test".into(),
        }));
        // Deref gives direct field access
        assert_eq!(t.name, "Test");
    }

    #[tokio::test]
    async fn extract_from_extensions() {
        let tenant = TestTenant {
            id: "t1".into(),
            name: "Test".into(),
        };
        let (mut parts, _) = http::Request::builder()
            .body(())
            .unwrap()
            .into_parts();
        parts.extensions.insert(Arc::new(tenant));

        let result = Tenant::<TestTenant>::from_request_parts(&mut parts, &()).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().get().id, "t1");
    }

    #[tokio::test]
    async fn extract_missing_returns_500() {
        let (mut parts, _) = http::Request::builder()
            .body(())
            .unwrap()
            .into_parts();

        let result = Tenant::<TestTenant>::from_request_parts(&mut parts, &()).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.status(), http::StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn option_tenant_none_when_missing() {
        let (mut parts, _) = http::Request::builder()
            .body(())
            .unwrap()
            .into_parts();

        let result =
            Option::<Tenant<TestTenant>>::from_request_parts(&mut parts, &()).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[tokio::test]
    async fn option_tenant_some_when_present() {
        let tenant = TestTenant {
            id: "t1".into(),
            name: "Test".into(),
        };
        let (mut parts, _) = http::Request::builder()
            .body(())
            .unwrap()
            .into_parts();
        parts.extensions.insert(Arc::new(tenant));

        let result =
            Option::<Tenant<TestTenant>>::from_request_parts(&mut parts, &()).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_some());
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib -- tenant::extractor::tests -v`
Expected: FAIL

- [ ] **Step 3: Implement Tenant extractor**

```rust
// src/tenant/extractor.rs
use std::ops::Deref;
use std::sync::Arc;

use axum::extract::FromRequestParts;
use http::request::Parts;

use crate::Error;

use super::traits::HasTenantId;

/// Extractor that provides access to the resolved tenant.
///
/// Pulls the resolved tenant from request extensions (inserted by tenant middleware).
/// Returns 500 if tenant middleware is not applied — this is a developer misconfiguration.
///
/// Use `Option<Tenant<T>>` for routes that work with or without a tenant.
pub struct Tenant<T>(pub(crate) Arc<T>);

impl<T> Tenant<T> {
    /// Returns a reference to the resolved tenant.
    pub fn get(&self) -> &T {
        &self.0
    }

    /// Returns the inner `Arc<T>`. Crate-internal only.
    pub(crate) fn into_inner(self) -> Arc<T> {
        self.0
    }
}

impl<T> Deref for Tenant<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T> Clone for Tenant<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T, S> FromRequestParts<S> for Tenant<T>
where
    T: HasTenantId + Send + Sync + Clone + 'static,
    S: Send + Sync,
{
    type Rejection = Error;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<Arc<T>>()
            .cloned()
            .map(Tenant)
            .ok_or_else(|| Error::internal("Tenant middleware not applied"))
    }
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test --lib -- tenant::extractor::tests -v`
Expected: all pass

- [ ] **Step 5: Commit**

```bash
git add src/tenant/extractor.rs
git commit -m "feat(tenant): implement Tenant<T> extractor with Deref and FromRequestParts"
```

---

### Task 8: Tenant middleware

**Files:**
- Create: `src/tenant/middleware.rs`

- [ ] **Step 1: Write tests**

Define test helpers first (a mock strategy and resolver), then test the middleware flow.

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use http::{Request, Response, StatusCode};
    use std::convert::Infallible;
    use tower::ServiceExt;

    #[derive(Clone, Debug)]
    struct TestTenant {
        id: String,
    }

    impl HasTenantId for TestTenant {
        fn tenant_id(&self) -> &str {
            &self.id
        }
    }

    struct OkStrategy;
    impl TenantStrategy for OkStrategy {
        fn extract(&self, _parts: &mut http::request::Parts) -> crate::Result<TenantId> {
            Ok(TenantId::Slug("acme".into()))
        }
    }

    struct FailStrategy;
    impl TenantStrategy for FailStrategy {
        fn extract(&self, _parts: &mut http::request::Parts) -> crate::Result<TenantId> {
            Err(Error::bad_request("no tenant"))
        }
    }

    struct OkResolver;
    impl TenantResolver for OkResolver {
        type Tenant = TestTenant;
        async fn resolve(&self, _id: &TenantId) -> crate::Result<TestTenant> {
            Ok(TestTenant { id: "t1".into() })
        }
    }

    struct NotFoundResolver;
    impl TenantResolver for NotFoundResolver {
        type Tenant = TestTenant;
        async fn resolve(&self, _id: &TenantId) -> crate::Result<TestTenant> {
            Err(Error::not_found("tenant not found"))
        }
    }

    /// Inner service that checks extensions for resolved tenant.
    async fn echo_handler(req: Request<Body>) -> Result<Response<Body>, Infallible> {
        let has_tenant = req.extensions().get::<Arc<TestTenant>>().is_some();
        let body = if has_tenant { "ok" } else { "no-tenant" };
        Ok(Response::new(Body::from(body)))
    }

    #[tokio::test]
    async fn strategy_ok_resolver_ok_passes_through() {
        let layer = TenantLayer::new(OkStrategy, OkResolver);
        let svc = layer.layer(tower::service_fn(echo_handler));

        let req = Request::builder().body(Body::empty()).unwrap();
        let resp = svc.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn strategy_fail_returns_400() {
        let layer = TenantLayer::new(FailStrategy, OkResolver);
        let svc = layer.layer(tower::service_fn(echo_handler));

        let req = Request::builder().body(Body::empty()).unwrap();
        let resp = svc.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn resolver_not_found_returns_404() {
        let layer = TenantLayer::new(OkStrategy, NotFoundResolver);
        let svc = layer.layer(tower::service_fn(echo_handler));

        let req = Request::builder().body(Body::empty()).unwrap();
        let resp = svc.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn tenant_in_extensions_after_resolve() {
        let layer = TenantLayer::new(OkStrategy, OkResolver);

        // Custom inner service that asserts tenant is in extensions
        let inner = tower::service_fn(|req: Request<Body>| async move {
            let tenant = req.extensions().get::<Arc<TestTenant>>().unwrap();
            assert_eq!(tenant.id, "t1");
            Ok::<_, Infallible>(Response::new(Body::empty()))
        });

        let svc = layer.layer(inner);
        let req = Request::builder().body(Body::empty()).unwrap();
        let resp = svc.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib -- tenant::middleware::tests -v`
Expected: FAIL

- [ ] **Step 3: Implement TenantLayer and TenantMiddleware**

```rust
// src/tenant/middleware.rs
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use axum::body::Body;
use http::Request;
use tower::{Layer, Service};

use crate::error::Error;

use super::traits::{HasTenantId, TenantResolver, TenantStrategy};
use super::TenantId;

/// Creates a tenant middleware layer from a strategy and resolver.
pub fn middleware<S, R>(strategy: S, resolver: R) -> TenantLayer<S, R>
where
    S: TenantStrategy,
    R: TenantResolver,
{
    TenantLayer::new(strategy, resolver)
}

/// Tower layer that produces `TenantMiddleware` services.
#[derive(Clone)]
pub struct TenantLayer<S, R> {
    strategy: Arc<S>,
    resolver: Arc<R>,
}

impl<S, R> TenantLayer<S, R> {
    pub fn new(strategy: S, resolver: R) -> Self {
        Self {
            strategy: Arc::new(strategy),
            resolver: Arc::new(resolver),
        }
    }
}

impl<Svc, S, R> Layer<Svc> for TenantLayer<S, R>
where
    S: TenantStrategy,
    R: TenantResolver,
{
    type Service = TenantMiddleware<Svc, S, R>;

    fn layer(&self, inner: Svc) -> Self::Service {
        TenantMiddleware {
            inner,
            strategy: self.strategy.clone(),
            resolver: self.resolver.clone(),
        }
    }
}

/// Tower service that resolves tenants from requests.
#[derive(Clone)]
pub struct TenantMiddleware<Svc, S, R> {
    inner: Svc,
    strategy: Arc<S>,
    resolver: Arc<R>,
}

impl<Svc, S, R> Service<Request<Body>> for TenantMiddleware<Svc, S, R>
where
    Svc: Service<Request<Body>, Response = http::Response<Body>> + Clone + Send + 'static,
    Svc::Future: Send + 'static,
    Svc::Error: Into<Box<dyn std::error::Error + Send + Sync>> + Send + 'static,
    S: TenantStrategy,
    R: TenantResolver,
{
    type Response = http::Response<Body>;
    type Error = Svc::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request<Body>) -> Self::Future {
        let strategy = self.strategy.clone();
        let resolver = self.resolver.clone();
        let mut inner = self.inner.clone();
        std::mem::swap(&mut self.inner, &mut inner);

        Box::pin(async move {
            let (mut parts, body) = request.into_parts();

            // Step 1: Extract tenant identifier
            let tenant_id = match strategy.extract(&mut parts) {
                Ok(id) => id,
                Err(e) => return Ok(e.into_response()),
            };

            // Step 2: Resolve tenant
            let tenant = match resolver.resolve(&tenant_id).await {
                Ok(t) => t,
                Err(e) => return Ok(e.into_response()),
            };

            // Step 3: Record tenant_id in tracing span
            tracing::Span::current().record(
                "tenant_id",
                tenant.tenant_id(),
            );

            // Step 4: Insert into extensions
            let tenant = Arc::new(tenant);
            parts.extensions.insert(tenant);

            let request = Request::from_parts(parts, body);
            inner.call(request).await
        })
    }
}
```

Key implementation notes:
- Uses `mem::swap` pattern from session middleware
- Errors returned via `Error::into_response()` — propagates through error handler middleware
- `tracing::Span::current().record()` fills in the `tenant_id` field on the request span
- Inserts `Arc<T>` into extensions (matched by extractor's `get::<Arc<T>>()`)

- [ ] **Step 4: Run tests**

Run: `cargo test --lib -- tenant::middleware::tests -v`
Expected: all pass

- [ ] **Step 5: Commit**

```bash
git add src/tenant/middleware.rs
git commit -m "feat(tenant): implement tenant middleware with strategy + resolver"
```

---

### Task 9: Tracing middleware update

**Files:**
- Modify: `src/middleware/tracing.rs`

The request tracing middleware must declare `tenant_id` as an `Empty` field so the tenant middleware can fill it in via `Span::current().record()`.

- [ ] **Step 1: Update tracing middleware**

```rust
// src/middleware/tracing.rs
use tower_http::classify::ServerErrorsAsFailures;
use tower_http::classify::SharedClassifier;
use tower_http::trace::{MakeSpan, TraceLayer};

/// Custom span maker that includes a `tenant_id` field for tenant middleware.
#[derive(Clone, Debug)]
pub struct ModoMakeSpan;

impl<B> MakeSpan<B> for ModoMakeSpan {
    fn make_span(&mut self, request: &http::Request<B>) -> tracing::Span {
        tracing::info_span!(
            "http_request",
            method = %request.method(),
            uri = %request.uri(),
            version = ?request.version(),
            tenant_id = tracing::field::Empty,
        )
    }
}

/// Returns a tracing layer configured for HTTP request/response lifecycle logging.
///
/// The span includes a `tenant_id` field (initially empty) that the tenant
/// middleware fills in after resolution.
pub fn tracing() -> TraceLayer<SharedClassifier<ServerErrorsAsFailures>, ModoMakeSpan> {
    TraceLayer::new_for_http().make_span_with(ModoMakeSpan)
}
```

- [ ] **Step 2: Verify it compiles and existing tests pass**

Run: `cargo check && cargo test`
Expected: compiles and all existing tests pass. The return type of `tracing()` changed, so check that all call sites still compile. `ModoMakeSpan` does not need to be re-exported from `middleware/mod.rs` — callers use the `tracing()` function which returns the layer; they never name `ModoMakeSpan` directly.

**Note on tracing span testing:** Verifying that `tenant_id` is recorded on the span is difficult in unit tests without a custom tracing subscriber. The tracing integration is verified indirectly — the middleware calls `Span::current().record()`, and the tracing middleware declares the empty field. If these two pieces compile and the middleware tests pass, the integration works. Manual verification can be done by running an app with the tenant middleware and checking log output.

- [ ] **Step 3: Commit**

```bash
git add src/middleware/tracing.rs
git commit -m "feat(middleware): add tenant_id Empty field to request tracing span"
```

---

### Task 10: Final integration and verification

**Files:**
- Verify: `src/tenant/mod.rs`, `src/lib.rs`

- [ ] **Step 1: Verify mod.rs re-exports are correct**

Read `src/tenant/mod.rs` and confirm all public types are re-exported. Update if needed based on actual module structure from implementation.

- [ ] **Step 2: Verify lib.rs re-exports**

Read `src/lib.rs` and confirm `pub mod tenant` is present with correct re-exports:
```rust
pub use tenant::{HasTenantId, Tenant, TenantId, TenantResolver, TenantStrategy};
```

- [ ] **Step 3: Run full test suite**

Run: `cargo test`
Expected: all tests pass (including existing tests from other modules)

- [ ] **Step 4: Run clippy**

Run: `cargo clippy --tests -- -D warnings`
Expected: no warnings

- [ ] **Step 5: Run format check**

Run: `cargo fmt --check`
Expected: no formatting issues

- [ ] **Step 6: Commit if any fixes were needed**

```bash
git add -A
git commit -m "fix(tenant): address clippy and formatting issues"
```

---

## CLAUDE.md Update

After all tasks complete, add to `CLAUDE.md`:

Under **Implementation Roadmap**:
```
- **Plan 9 (Tenant):** tenant resolution with strategies, resolver trait, middleware enforcement — DONE
```

Under **Gotchas** (if any new gotchas discovered during implementation).
