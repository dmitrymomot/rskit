# modo v2 — Tenant Resolution Design

Multi-tenant resolution for modo v2. Extracts tenant identity from HTTP requests, resolves to app-defined types via async DB lookup, enforces tenant presence at the middleware level.

## Intentional Departures from Master Design Spec

- Resolver is a separate `TenantResolver` trait, not a closure chained onto the strategy (`.resolve(|slug, registry| ...)` in the master spec). Separating strategy from resolver makes each independently testable and reusable.
- Resolver holds its own dependencies (e.g., `ReadPool`), not receiving `&Registry` as a parameter. Consistent with how other modo modules work (services own their deps).
- No `Tenant<String>` fallback without resolver. Resolution is always explicit — either provide a resolver or don't use tenant middleware.
- Configuration is programmatic, not via YAML `TenantConfig`. Strategy selection and resolver wiring require code (trait impls, struct construction) that can't be expressed in config. The app passes strategy parameters (base domain, header name) from its own config.

## Scope

**In scope:**
- 7 identification strategies (subdomain, domain, combined, header, api_key_header, path_prefix, path_param)
- `TenantId` enum with 4 variants (Slug, Domain, Id, ApiKey)
- `TenantResolver` trait — app implements lookup query
- `HasTenantId` trait — resolved type must implement for tracing
- Middleware-based enforcement (errors propagate through error handler)
- `Tenant<T>` / `Option<Tenant<T>>` extractors
- Auto `tenant_id` tracing span field
- Always available (no feature flag)

**Out of scope:**
- Caching
- Strategy chaining/fallback
- Data isolation helpers
- Tenant-scoped sessions
- CRUD operations
- Provisioning hooks

## Types

### `TenantId`

Represents the extracted identifier from the request. Each strategy produces a specific variant.

```rust
pub enum TenantId {
    /// From subdomain, path_prefix, path_param
    Slug(String),
    /// From domain(), combined strategy's domain branch
    Domain(String),
    /// From header() — generic identifier
    Id(String),
    /// From api_key_header() — raw API key, app should hash before lookup
    ApiKey(String),
}
```

**Display:** `Slug("acme")` → `"slug:acme"`, `Domain("acme.com")` → `"domain:acme.com"`, `Id("abc123")` → `"id:abc123"`, `ApiKey(_)` → `"apikey:[REDACTED]"` (never log raw API keys).

**Debug:** Same as Display — `ApiKey` always redacted.

**Helper method:** `TenantId::as_str() -> &str` returns the inner string regardless of variant. Useful when the app uses a single strategy and doesn't need to match on variants.

### `HasTenantId`

Resolved type must implement this trait. Used by middleware to populate the tracing span.

```rust
pub trait HasTenantId {
    fn tenant_id(&self) -> &str;
}
```

### `TenantResolver`

App implements this trait with their DB lookup logic. Uses RPITIT (like `OAuthProvider`) — not object-safe, resolver is a concrete type registered at startup.

```rust
pub trait TenantResolver: Send + Sync + 'static {
    type Tenant: HasTenantId + Send + Sync + Clone + 'static;

    fn resolve(&self, id: &TenantId) -> impl Future<Output = Result<Self::Tenant>> + Send;
}
```

Note: the generic `R: TenantResolver` propagates through the layer/service types. This is acceptable — same pattern as axum's own typed layers.

### `Tenant<T>`

Extractor that pulls the resolved tenant from request extensions.

```rust
pub struct Tenant<T>(Arc<T>);

impl<T> Tenant<T> {
    pub fn get(&self) -> &T { &self.0 }
    pub(crate) fn into_inner(self) -> Arc<T> { self.0 }
}

impl<T> Deref for Tenant<T> {
    type Target = T;
    fn deref(&self) -> &T { &self.0 }
}
```

`into_inner` is `pub(crate)` — does not leak `Arc` to downstream users. `Deref` provides ergonomic field access: `tenant.name` instead of `tenant.get().name`.

## Strategies

A strategy extracts `TenantId` from request parts. Takes `&mut Parts` to allow URI rewriting (needed by `path_prefix`).

```rust
pub trait TenantStrategy: Send + Sync + 'static {
    fn extract(&self, req: &mut http::request::Parts) -> Result<TenantId>;
}
```

All strategies strip port from `Host` header before processing (e.g., `acme.app.com:8080` → `acme.app.com`).

### `subdomain(base_domain)` → `SubdomainStrategy`

Parses `Host` header, strips base domain, returns `TenantId::Slug`.

- `acme.app.com` with base `acme.com` → `Slug("app")`
- Only one subdomain level allowed relative to base domain. `test.app.acme.com` with base `acme.com` → Error (multi-level subdomain)
- Base domain itself can be multi-level: `test.app.acme.com` with base `app.acme.com` → `Slug("test")`
- Error if Host equals base domain (no subdomain) or missing

### `domain()` → `DomainStrategy`

Returns full `Host` header value as `TenantId::Domain`.

- `acme.com` → `Domain("acme.com")`
- Error if Host missing

### `subdomain_or_domain(base_domain)` → `SubdomainOrDomainStrategy`

Combined strategy. Checks if Host is a subdomain of the base domain.

- If Host is exactly one level above base domain → `Slug(subdomain)`
- If Host does not end with base domain → `Domain(full_host)` (custom domain)
- `app.acme.com` with base `acme.com` → `Slug("app")`
- `custom.com` → `Domain("custom.com")`
- Error if Host equals base domain exactly (no subdomain, not a custom domain — invalid for tenant routes)
- Error if Host is multi-level subdomain of base domain (`test.app.acme.com` with base `acme.com` — invalid)
- Error if Host missing

### `header(name)` → `HeaderStrategy`

Reads named request header, returns `TenantId::Id`.

- `X-Tenant-Id: abc123` → `Id("abc123")`
- Error if header missing or not valid UTF-8

### `api_key_header(name)` → `ApiKeyHeaderStrategy`

Reads named request header, returns `TenantId::ApiKey`.

- `X-Api-Key: sk_live_...` → `ApiKey("sk_live_...")`
- Error if header missing or not valid UTF-8
- App is responsible for hashing before DB lookup

### `path_prefix(prefix)` → `PathPrefixStrategy`

Extracts first segment after prefix, rewrites request URI to strip prefix + segment.

- `/t/acme/dashboard` with prefix `"/t"` → `Slug("acme")`, URI rewritten to `/dashboard`
- `/t/acme` with prefix `"/t"` → `Slug("acme")`, URI rewritten to `/`
- Error if path doesn't start with prefix or no segment after it
- URI rewriting uses `&mut Parts` — strategy modifies `parts.uri` directly

### `path_param(name)` → `PathParamStrategy`

Reads named axum path parameter, returns `TenantId::Slug`. No path modification — routes must include the param in their pattern.

- `/{tenant}/dashboard` with param name `"tenant"` → `Slug("acme")`
- Error if param not found (500 — misconfiguration)
- **Requires `.route_layer()`** instead of `.layer()` — path params are only available after axum route matching. Using `.layer()` will cause the param to be missing.

## Middleware

### Construction

```rust
let middleware = tenant::middleware(strategy, resolver);
```

Applied to a router group via `.layer()` (or `.route_layer()` for `path_param` strategy). All routes in the group require a valid tenant.

### Request flow

1. Strategy extracts `TenantId` from request parts (may rewrite URI)
2. If extraction fails → return `Error::bad_request("...")`
3. Resolver called with `&TenantId` → async DB lookup
4. If resolver returns `Err` → error propagates through error handler (typically 404)
5. If resolver returns `Ok(tenant)` →
   - Insert `Arc<T>` into request extensions
   - Add `tenant_id` field to current tracing span (via `HasTenantId::tenant_id()`)
   - Call inner service

### Error handling

All errors are `modo::Error` — they propagate through the error handler middleware like any other error. No special error path.

| Failure | Error |
|---|---|
| Strategy can't extract identifier | `bad_request` (400) |
| Resolver returns error (not found) | Whatever the resolver returns (app decides — typically 404) |
| Resolver returns error (DB failure) | `internal` (500) |

### Layer type

Standard tower `Layer` + `Service` pattern, same as session/CSRF middleware.

```rust
pub struct TenantLayer<S, R> { strategy: Arc<S>, resolver: Arc<R> }
pub struct TenantService<Svc, S, R> { inner: Svc, strategy: Arc<S>, resolver: Arc<R> }
```

`S: TenantStrategy`, `R: TenantResolver`. Both wrapped in `Arc` for cheap cloning per-request.

### Tracing integration

After successful resolution, the middleware records the tenant identity on the current request span:

```rust
tracing::Span::current().record("tenant_id", tenant.tenant_id());
```

This uses `HasTenantId::tenant_id()` on the resolved value. The field appears automatically in all downstream log lines for the request — handlers and other middleware don't need to add it manually.

The request tracing middleware (from Plan 2) must declare the `tenant_id` field as `Empty` in its span so the tenant middleware can fill it in later. If the tracing middleware doesn't declare the field, `record()` silently drops it.

## Extractor

### `Tenant<T>` — required

```rust
impl<T, S> FromRequestParts<S> for Tenant<T>
where
    T: HasTenantId + Send + Sync + Clone + 'static,
    S: Send + Sync,
{
    type Rejection = Error;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self> {
        parts.extensions.get::<Arc<T>>()
            .cloned()
            .map(Tenant)
            .ok_or_else(|| Error::internal("Tenant middleware not applied"))
    }
}
```

Returns 500 if tenant not in extensions — developer misconfiguration.

### `Option<Tenant<T>>` — optional

Works via axum's blanket impl for `Option<T> where T: FromRequestParts`. Returns `None` if tenant not in extensions.

Useful for routes that work with or without tenant context.

### Handler usage

```rust
// Required — 500 if middleware missing, never reached if tenant not found (middleware returns 404)
async fn dashboard(tenant: Tenant<Org>) -> Result<impl IntoResponse> {
    let name = &tenant.name; // Deref access
    let org = tenant.get();  // explicit access
}

// Optional — None if no tenant middleware on this route
async fn landing(tenant: Option<Tenant<Org>>) -> Result<impl IntoResponse> {
    if let Some(tenant) = tenant {
        // personalized
    } else {
        // generic
    }
}
```

## File Structure

```
src/tenant/
    mod.rs          — mod imports + re-exports
    id.rs           — TenantId enum, Display, Debug, as_str()
    traits.rs       — HasTenantId, TenantResolver, TenantStrategy traits
    strategy.rs     — all built-in strategy structs + constructor functions
    middleware.rs   — TenantLayer, TenantService, tenant::middleware() constructor
    extractor.rs    — Tenant<T> struct + Deref + FromRequestParts impl
```

## Public API

```rust
// Types
pub use tenant::TenantId;
pub use tenant::Tenant;

// Traits
pub use tenant::HasTenantId;
pub use tenant::TenantResolver;
pub use tenant::TenantStrategy;

// Strategy constructors (return concrete types that impl TenantStrategy)
tenant::subdomain("app.com")           // → SubdomainStrategy
tenant::domain()                       // → DomainStrategy
tenant::subdomain_or_domain("app.com") // → SubdomainOrDomainStrategy
tenant::header("X-Tenant-Id")          // → HeaderStrategy
tenant::api_key_header("X-Api-Key")    // → ApiKeyHeaderStrategy
tenant::path_prefix("/t")             // → PathPrefixStrategy
tenant::path_param("tenant")          // → PathParamStrategy

// Middleware constructor
tenant::middleware(strategy, resolver)
```

## End-to-End Example

```rust
// App types
struct Org { id: String, name: String, slug: String }

impl HasTenantId for Org {
    fn tenant_id(&self) -> &str { &self.id }
}

struct OrgResolver { db: ReadPool }

impl TenantResolver for OrgResolver {
    type Tenant = Org;

    async fn resolve(&self, id: &TenantId) -> Result<Org> {
        let pool = &*self.db;
        match id {
            TenantId::Slug(slug) => {
                sqlx::query_as("SELECT id, name, slug FROM orgs WHERE slug = ?")
                    .bind(slug).fetch_optional(pool).await?
            }
            TenantId::Domain(domain) => {
                sqlx::query_as("SELECT id, name, slug FROM orgs WHERE custom_domain = ?")
                    .bind(domain).fetch_optional(pool).await?
            }
            TenantId::ApiKey(key) => {
                let key_hash = sha256(key);
                sqlx::query_as(
                    "SELECT o.id, o.name, o.slug FROM orgs o
                     JOIN api_keys k ON k.tenant_id = o.id WHERE k.key_hash = ?"
                ).bind(key_hash).fetch_optional(pool).await?
            }
            TenantId::Id(id) => {
                sqlx::query_as("SELECT id, name, slug FROM orgs WHERE id = ?")
                    .bind(id).fetch_optional(pool).await?
            }
        }
        .ok_or(Error::not_found("Tenant not found"))
    }
}

// main()
let resolver = OrgResolver { db: read_pool.clone() };
let strategy = tenant::subdomain_or_domain("app.com");
let tenant_layer = tenant::middleware(strategy, resolver);

let tenant_routes = Router::new()
    .route("/dashboard", get(dashboard))
    .route("/settings", get(settings));

let app = Router::new()
    .route("/", get(landing))                         // no tenant
    .nest("/app", tenant_routes.layer(tenant_layer)); // tenant required
```

## Testing Strategy

### Unit tests (in-crate `#[cfg(test)] mod tests`)

**`id.rs`** — `TenantId` enum:
- Display formatting (slug/domain/id show value, apikey redacted)
- Debug formatting (apikey redacted)
- `as_str()` returns inner value for all variants
- Equality comparisons

**`strategy.rs`** — each strategy:
- `subdomain`: valid single-level subdomain, multi-level subdomain (error), bare base domain (error), missing Host (error), port stripping, multi-level base domain with valid subdomain
- `domain`: valid Host, missing Host (error), port stripping
- `subdomain_or_domain`: subdomain branch (single-level), custom domain branch, exact base domain (error), multi-level subdomain (error), missing Host (error)
- `header` / `api_key_header`: present header, missing header (error), non-UTF-8 (error)
- `path_prefix`: valid prefix with segment, prefix only (→ `/`), missing segment (error), wrong prefix (error), URI rewriting verification
- `path_param`: present param, missing param (error)

**`middleware.rs`** — request flow:
- Strategy succeeds + resolver succeeds → inner service called, tenant in extensions, tracing span has `tenant_id`
- Strategy fails → 400 returned, inner service not called
- Resolver returns not found → 404 propagated, inner service not called
- Resolver returns internal error → 500 propagated

**`extractor.rs`** — `Tenant<T>`:
- Tenant in extensions → extracts successfully
- Deref access to inner fields
- Tenant not in extensions → 500 error
- `Option<Tenant<T>>` without extensions → `None`
