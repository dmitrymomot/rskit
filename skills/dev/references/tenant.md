# Multi-Tenancy

Module: `src/tenant/` | Always available (no feature gate)

Import types from `modo::tenant`: `HasTenantId`, `Tenant`, `TenantId`, `TenantLayer`, `TenantResolver`, `TenantStrategy`, `middleware` (the free function — also available as `modo::middlewares::tenant`).

Domain submodule exports: `modo::tenant::domain::{ClaimStatus, DomainClaim, DomainService, TenantMatch}`.

## Overview

The tenant system has four moving parts:

1. A **TenantStrategy** extracts a `TenantId` from `http::request::Parts`.
2. A **TenantResolver** maps that `TenantId` to the app's concrete tenant type.
3. **TenantLayer / TenantMiddleware** (Tower middleware) wires the two together.
4. The **Tenant\<T\>** extractor gives handlers access to the resolved tenant.

## TenantId

```rust
#[derive(Clone, PartialEq, Eq)]
pub enum TenantId {
    Slug(String),    // subdomain, path_prefix, path_param
    Domain(String),  // domain(), subdomain_or_domain (custom domain branch)
    Id(String),      // header()
    ApiKey(String),  // api_key_header()
}
```

Methods:

- `as_str(&self) -> &str` -- returns the inner string regardless of variant.

`Display` output: `slug:acme`, `domain:acme.com`, `id:abc123`, `apikey:[REDACTED]`.
`Debug` output: `ApiKey` variant prints `ApiKey("[REDACTED]")` -- the raw key is never exposed.

## TenantStrategy trait

```rust
pub trait TenantStrategy: Send + Sync + 'static {
    fn extract(&self, parts: &mut http::request::Parts) -> Result<TenantId>;
}
```

Takes `&mut Parts` because some strategies rewrite the URI (e.g. `PathPrefixStrategy`).

## Built-in strategies

| Constructor                        | Struct                      | Produces           | Notes                                                                          |
| ---------------------------------- | --------------------------- | ------------------ | ------------------------------------------------------------------------------ |
| `subdomain(base_domain)`           | `SubdomainStrategy`         | `TenantId::Slug`   | Single-level only; multi-level is rejected                                     |
| `domain()`                         | `DomainStrategy`            | `TenantId::Domain` | Full host as identifier                                                        |
| `subdomain_or_domain(base_domain)` | `SubdomainOrDomainStrategy` | `Slug` or `Domain` | Subdomain of base -> Slug; unrelated host -> Domain; bare base domain -> error |
| `header(name)`                     | `HeaderStrategy`            | `TenantId::Id`     | Reads a named request header                                                   |
| `api_key_header(name)`             | `ApiKeyHeaderStrategy`      | `TenantId::ApiKey` | Reads API key from a named header                                              |
| `path_prefix(prefix)`              | `PathPrefixStrategy`        | `TenantId::Slug`   | Strips prefix + slug from URI, preserves query string                          |
| `path_param(name)`                 | `PathParamStrategy`         | `TenantId::Slug`   | Reads axum path parameter; **must use `.route_layer()` not `.layer()`**        |

All constructors are free functions in `modo::tenant` (also re-exported from `modo::tenant::*`).

## TenantResolver trait

```rust
pub trait TenantResolver: Send + Sync + 'static {
    type Tenant: HasTenantId + Send + Sync + Clone + 'static;
    fn resolve(&self, id: &TenantId) -> impl Future<Output = Result<Self::Tenant>> + Send;
}
```

Uses RPITIT (return-position `impl Trait` in trait) -- **not object-safe**. The resolver must be a concrete type, not `Arc<dyn TenantResolver>`.

## HasTenantId trait

```rust
pub trait HasTenantId {
    fn tenant_id(&self) -> &str;
}
```

Required on the resolved tenant type. The middleware calls `tenant.tenant_id()` to record the value in the current tracing span.

## TenantLayer and TenantMiddleware

Create via the `middleware()` function (re-exported as `modo::tenant_middleware`):

```rust
pub fn middleware<S, R>(strategy: S, resolver: R) -> TenantLayer<S, R>
where
    S: TenantStrategy,
    R: TenantResolver;
```

`TenantLayer` is a Tower `Layer` that produces `TenantMiddleware` services. It also exposes a public constructor:

```rust
impl<S, R> TenantLayer<S, R> {
    pub fn new(strategy: S, resolver: R) -> Self;
}
```

`TenantMiddleware<Svc, S, R>` is a Tower `Service` (no public methods beyond the `Service` impl). Both `TenantLayer` and `TenantMiddleware` implement `Clone`.

On each request the middleware:

1. Calls `strategy.extract(&mut parts)` to get a `TenantId`.
2. Calls `resolver.resolve(&tenant_id).await` to get the concrete tenant.
3. Records `tenant_id` in the current tracing span via `Span::current().record()`.
4. Inserts the resolved tenant as `Arc<T>` into request extensions.

Errors at step 1 or 2 short-circuit -- the inner service is never called and the error is converted to an HTTP response via `Error::into_response()`.

## Tenant\<T\> extractor

```rust
pub struct Tenant<T>(/* Arc<T> */);
```

Implements `FromRequestParts`, `OptionalFromRequestParts`, `Deref<Target = T>`, `Clone`, and `Debug` (when `T: Debug`).

- `Tenant<T>` -- returns 500 if middleware is not applied (developer error).
- `Option<Tenant<T>>` -- returns `None` if no tenant in extensions.
- `.get() -> &T` for an explicit reference.
- Dereferences to `T` so fields are accessible directly via `Deref`.

## Domain submodule (`tenant::domain`)

Always available.

Provides domain claim registration, DNS-based verification, and domain-to-tenant lookups.

### ClaimStatus

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ClaimStatus {
    Pending,
    Verified,
    Failed,
}
```

Methods:

- `as_str(&self) -> &'static str` -- returns `"pending"`, `"verified"`, or `"failed"`.

### DomainClaim

```rust
#[derive(Debug, Clone, Serialize)]
pub struct DomainClaim {
    pub id: String,
    pub tenant_id: String,
    pub domain: String,
    pub verification_token: String,
    pub status: ClaimStatus,
    pub use_for_email: bool,
    pub use_for_routing: bool,
    pub created_at: String,
    pub verified_at: Option<String>,
}
```

### TenantMatch

```rust
#[derive(Debug, Clone, Serialize)]
pub struct TenantMatch {
    pub tenant_id: String,
    pub domain: String,
}
```

### Free functions

```rust
pub fn validate_domain(domain: &str) -> Result<String>;
pub fn extract_email_domain(email: &str) -> Result<String>;
```

- `validate_domain` -- trims, lowercases, rejects empty/no-dot/leading-trailing dots or hyphens/labels >63 chars/total >253 chars/non-alphanumeric-hyphen chars.
- `extract_email_domain` -- splits on `@`, validates the domain portion via `validate_domain`.

### DomainService

```rust
#[derive(Clone)]
pub struct DomainService { /* Arc<Inner> */ }
```

Cheap to clone (`Arc` internally). Backed by `Database` and `DomainVerifier`.

```rust
impl DomainService {
    pub fn new(db: Database, verifier: DomainVerifier) -> Self;
    pub async fn register(&self, tenant_id: &str, domain: &str) -> Result<DomainClaim>;
    pub async fn verify(&self, id: &str) -> Result<DomainClaim>;
    pub async fn remove(&self, id: &str) -> Result<()>;
    pub async fn enable_email(&self, id: &str) -> Result<()>;
    pub async fn disable_email(&self, id: &str) -> Result<()>;
    pub async fn enable_routing(&self, id: &str) -> Result<()>;
    pub async fn disable_routing(&self, id: &str) -> Result<()>;
    pub async fn lookup_email_domain(&self, email: &str) -> Result<Option<TenantMatch>>;
    pub async fn lookup_routing_domain(&self, domain: &str) -> Result<Option<TenantMatch>>;
    pub async fn resolve_tenant(&self, domain: &str) -> Result<Option<String>>;
    pub async fn list(&self, tenant_id: &str) -> Result<Vec<DomainClaim>>;
}
```

Method details:

- `register` -- validates domain, returns existing pending claim if one exists (deduplication), generates a verification token, inserts a pending claim. Caller instructs user to create DNS TXT record at `_modo-verify.{domain}`.
- `verify` -- fetches claim by ID, returns already-verified claims immediately, checks 48-hour expiry (marks `failed` if expired), queries DNS TXT via `DomainVerifier::check_txt`, updates status to `verified` on success.
- `remove` -- deletes the claim row by ID.
- `enable_email` / `disable_email` -- toggles `use_for_email` flag. Enable requires verified status.
- `enable_routing` / `disable_routing` -- toggles `use_for_routing` flag. Enable requires verified status.
- `lookup_email_domain` -- extracts domain from email address, finds a verified domain with `use_for_email = true`.
- `lookup_routing_domain` -- validates domain, finds a verified domain with `use_for_routing = true`.
- `resolve_tenant` -- convenience wrapper around `lookup_routing_domain`, returns only the tenant ID.
- `list` -- returns all claims for a tenant ordered by `created_at DESC`. Pending claims older than 48 hours are returned with `Failed` status (computed in-memory).

Expected table: `tenant_domains` with columns `id`, `tenant_id`, `domain`, `verification_token`, `status`, `use_for_email`, `use_for_routing`, `created_at`, `verified_at`.

## Usage example

```rust
use modo::tenant::{self, HasTenantId, TenantId, TenantResolver, Tenant};

#[derive(Clone, Debug)]
struct Org {
    id: String,
    name: String,
}

impl HasTenantId for Org {
    fn tenant_id(&self) -> &str {
        &self.id
    }
}

struct OrgResolver { /* db pool, etc. */ }

impl TenantResolver for OrgResolver {
    type Tenant = Org;
    async fn resolve(&self, id: &TenantId) -> modo::Result<Org> {
        // Look up by id.as_str() in DB
        todo!()
    }
}

// In main():
let tenant_layer = tenant::middleware(
    tenant::subdomain("myapp.com"),
    OrgResolver { /* ... */ },
);

let app = axum::Router::new()
    .route("/dashboard", axum::routing::get(dashboard))
    .layer(tenant_layer);

async fn dashboard(tenant: Tenant<Org>) -> String {
    format!("Welcome to {}", tenant.name)
}
```

For `path_param`, use `.route_layer()` instead of `.layer()`:

```rust
let tenant_layer = tenant::middleware(
    tenant::path_param("org"),
    OrgResolver { /* ... */ },
);

let app = axum::Router::new()
    .route("/{org}/dashboard", axum::routing::get(dashboard))
    .route_layer(tenant_layer);
```

## Gotchas

- **ApiKey redaction**: `TenantId::ApiKey` is redacted in both `Display` and `Debug`. Never log the raw key. Use `as_str()` only when you need the actual value (e.g., for DB lookup).
- **Tracing span**: The middleware records `tenant_id` via `Span::current().record()`. For this to work, the enclosing tracing span (from `tracing()` middleware) must declare `tenant_id = tracing::field::Empty` -- spans without that field silently ignore the `record()` call.
- **PathParamStrategy requires `.route_layer()`**: Path parameters are only available after axum route matching. Using `.layer()` instead of `.route_layer()` will produce a 500 error.
- **TenantResolver is not object-safe**: It uses RPITIT, so you cannot use `Arc<dyn TenantResolver>`. Pass the concrete resolver type directly.
- **PathPrefixStrategy rewrites the URI**: The prefix and slug are stripped from the path; downstream handlers see the remaining path. Query strings are preserved.
- **Subdomain strategies reject multi-level subdomains**: `a.b.acme.com` against base `acme.com` is an error, not a valid tenant.
- **Domain enable requires verified**: `enable_email` and `enable_routing` return an error if the domain claim is not in `verified` status.
- **48-hour verification window**: Pending claims expire after 48 hours. `verify()` persists the `failed` status; `list()` computes it in-memory without persisting.
