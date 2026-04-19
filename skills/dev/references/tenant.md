# Multi-Tenancy and Tier Gating

Modules: `src/tenant/`, `src/tier/` | Always available

Tenant exports (`modo::tenant`): `HasTenantId`, `Tenant`, `TenantId`, `TenantLayer`, `TenantMiddleware`, `TenantResolver`, `TenantStrategy`, `middleware` (free function — also `modo::middlewares::tenant`), plus the strategy structs and constructors below.

Domain submodule (`modo::tenant::domain`): `ClaimStatus`, `DomainClaim`, `DomainService`, `TenantMatch`, `validate_domain`, `extract_email_domain`.

Tier exports (`modo::tier`): `FeatureAccess`, `TierBackend`, `TierInfo`, `TierLayer` (also `modo::middlewares::Tier`), `TierResolver`, `require_feature`, `require_limit` (also `modo::guards::require_feature`, `modo::guards::require_limit`). Test-only helpers under `modo::tier::test`: `StaticTierBackend`, `FailingTierBackend` (gated on `test-helpers`).

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

Create via the `middleware()` function (re-exported as `modo::middlewares::tenant`):

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

Implements `FromRequestParts`, `OptionalFromRequestParts`, `Deref<Target = T>`, `Clone`, and `Debug` (when `T: Debug`). Both extractor impls require `T: HasTenantId + Send + Sync + Clone + 'static`.

- `Tenant<T>` -- returns 500 if middleware is not applied (developer error).
- `Option<Tenant<T>>` -- returns `None` if no tenant in extensions.
- `.get() -> &T` for an explicit reference.
- Dereferences to `T` so fields are accessible directly via `Deref`.

## Domain submodule (`modo::tenant::domain`)

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

## Tier (`modo::tier`)

Tier-based feature gating for SaaS apps. The framework provides the trait, wrapper, middleware, and guards; the app implements its own backend.

### `modo::tier::FeatureAccess`

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeatureAccess {
    Toggle(bool),
    Limit(u64),
}
```

### `modo::tier::TierInfo`

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TierInfo {
    pub name: String,
    pub features: HashMap<String, FeatureAccess>,
}
```

Methods:

- `has_feature(&self, name: &str) -> bool` -- `Toggle(true)` or `Limit(>0)`.
- `is_enabled(&self, name: &str) -> bool` -- `Toggle(true)` only.
- `limit(&self, name: &str) -> Option<u64>` -- ceiling for `Limit` features.
- `limit_ceiling(&self, name: &str) -> modo::Result<u64>` -- 403 if missing, 500 if `Toggle`.
- `check_limit(&self, name: &str, current: u64) -> modo::Result<()>` -- 403 if `current >= ceiling` or feature missing, 500 if `Toggle`.

Implements `FromRequestParts` (500 if `TierLayer` not applied) and `OptionalFromRequestParts` (`Ok(None)` if absent). Use as a handler argument: `tier: modo::tier::TierInfo` or `tier: Option<modo::tier::TierInfo>`.

### `modo::tier::TierBackend`

```rust
pub trait TierBackend: Send + Sync {
    fn resolve(
        &self,
        owner_id: &str,
    ) -> Pin<Box<dyn Future<Output = modo::Result<TierInfo>> + Send + '_>>;
}
```

Object-safe (uses `Pin<Box<dyn Future>>`, not RPITIT). The app implements this with its own DB/cache/HTTP logic.

### `modo::tier::TierResolver`

```rust
#[derive(Clone)]
pub struct TierResolver(/* Arc<dyn TierBackend> */);
```

Cheap to clone. Constructor and method:

- `TierResolver::from_backend(backend: Arc<dyn TierBackend>) -> Self`
- `async fn resolve(&self, owner_id: &str) -> modo::Result<TierInfo>`

### `modo::tier::TierLayer`

Tower middleware that resolves `TierInfo` and inserts it into request extensions. Re-exported as `modo::middlewares::Tier`.

```rust
impl TierLayer {
    pub fn new<F>(resolver: TierResolver, extractor: F) -> Self
    where
        F: Fn(&http::request::Parts) -> Option<String> + Send + Sync + 'static;

    pub fn with_default(self, default: TierInfo) -> Self;
}
```

Behavior:

- Extractor returns `Some(owner_id)` -- calls `resolver.resolve(owner_id)`; backend errors short-circuit with `Error::into_response()`.
- Extractor returns `None` and a default is set -- inserts the default `TierInfo`.
- Extractor returns `None` and no default -- no `TierInfo` inserted; downstream guards/extractors handle the absence (500 for required `TierInfo`, `Ok(None)` for `Option<TierInfo>`).

Apply with `.layer()` on the router so it runs before route matching. Guards apply with `.route_layer()`.

### `modo::guards::require_feature`

```rust
pub fn require_feature(name: &str) -> RequireFeatureLayer;
```

Tower layer that rejects requests when the resolved tier lacks the named feature.

- `TierInfo` missing in extensions -> 500 `Error::internal` (developer error: `TierLayer` not applied).
- Feature missing or `Toggle(false)` or `Limit(0)` -> 403 `Error::forbidden`.

Apply with `.route_layer()`.

### `modo::guards::require_limit`

```rust
pub fn require_limit<F, Fut>(name: &str, usage: F) -> RequireLimitLayer<F>
where
    F: Fn(&http::request::Parts) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = modo::Result<u64>> + Send;
```

Tower layer that calls `usage(&parts)` to get the current count, then rejects when `current >= ceiling`.

- `TierInfo` missing -> 500 `Error::internal`.
- Feature is `Toggle` (not a limit) -> 500 `Error::internal`.
- Feature missing -> 403 `Error::forbidden`.
- Ceiling is 0 -> 403 `Error::forbidden` ("not available on your current plan").
- `usage` closure returns `Err` -> error response via `Error::into_response()`.
- `current >= ceiling` -> 403 `Error::forbidden` ("Limit exceeded for '...': N/M").

Apply with `.route_layer()`. There is no 402 status -- all gating uses 403.

### Tier wiring example

```rust
use std::sync::Arc;
use axum::{Router, routing::get};
use modo::middlewares as mw;
use modo::tier::{TierBackend, TierResolver, TierInfo};
use modo::guards;

struct MyBackend { /* db pool */ }
impl TierBackend for MyBackend {
    fn resolve(
        &self,
        owner_id: &str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = modo::Result<TierInfo>> + Send + '_>> {
        Box::pin(async move { todo!() })
    }
}

let resolver = TierResolver::from_backend(Arc::new(MyBackend { /* ... */ }));

let app: Router<()> = Router::new()
    .route("/api/export", get(|| async { "ok" }))
    .route_layer(guards::require_feature("export"))
    .route("/api/calls", get(|| async { "ok" }))
    .route_layer(guards::require_limit("api_calls", |_parts| async { Ok(123u64) }))
    .layer(mw::Tier::new(resolver, |parts| {
        parts
            .extensions
            .get::<modo::tenant::TenantId>()
            .map(|id| id.as_str().to_owned())
    }));
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
- **TierLayer placement**: Apply `TierLayer` with `.layer()` so `TierInfo` is in extensions before guards run. `require_feature` / `require_limit` use `.route_layer()` and rely on it being set upstream.
- **Tier extractor needs the layer**: Handlers taking `TierInfo` directly return 500 if `TierLayer` is missing; use `Option<TierInfo>` to tolerate absence.
- **Guards return 403, not 402**: Both `require_feature` and `require_limit` reject with `Error::forbidden` (HTTP 403). Missing `TierInfo` is a 500 (developer misconfiguration).
- **`require_limit` ceiling=0**: A `Limit(0)` feature is treated as "not available on your current plan" (403) -- not "limit exceeded".
- **TierBackend is object-safe; TierResolver is concrete**: `TierBackend` uses `Pin<Box<dyn Future>>` so it works behind `Arc<dyn TierBackend>`. Wrap it in `TierResolver::from_backend(Arc::new(...))` for `TierLayer::new`.
- **Owner extractor reads sync `&Parts`**: The `TierLayer` extractor closure is sync. Use upstream middleware (e.g. `tenant`, session) to populate the source data into extensions, then read from `parts.extensions`.
