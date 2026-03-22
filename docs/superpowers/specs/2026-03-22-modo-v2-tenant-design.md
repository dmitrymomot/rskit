# modo v2 — Tenant Resolution Design

Multi-tenant resolution for modo v2. Extracts tenant identity from HTTP requests, resolves to app-defined types via async DB lookup, enforces tenant presence at the middleware level.

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

### `Tenant<T>`

Extractor that pulls the resolved tenant from request extensions.

```rust
pub struct Tenant<T>(Arc<T>);

impl<T> Tenant<T> {
    pub fn get(&self) -> &T { &self.0 }
    pub fn into_inner(self) -> Arc<T> { self.0 }
}
```

## Strategies

A strategy extracts `TenantId` from request parts.

```rust
pub trait TenantStrategy: Send + Sync + 'static {
    fn extract(&self, req: &http::request::Parts) -> Result<TenantId>;
}
```

### `subdomain(base_domain)`

Parses `Host` header, strips base domain, returns `TenantId::Slug`.

- `acme.app.com` → `Slug("acme")`
- Error if Host equals base domain (no subdomain) or missing

### `domain()`

Returns full `Host` header value as `TenantId::Domain`.

- `acme.com` → `Domain("acme.com")`
- Error if Host missing

### `subdomain_or_domain(base_domain)`

Combined strategy. Checks if Host is a subdomain of the base domain.

- If yes → `Slug(subdomain)`
- If no → `Domain(full_host)`
- `acme.app.com` → `Slug("acme")`, `custom.com` → `Domain("custom.com")`
- Error if Host missing

### `header(name)`

Reads named request header, returns `TenantId::Id`.

- `X-Tenant-Id: abc123` → `Id("abc123")`
- Error if header missing or not valid UTF-8

### `api_key_header(name)`

Reads named request header, returns `TenantId::ApiKey`.

- `X-Api-Key: sk_live_...` → `ApiKey("sk_live_...")`
- Error if header missing or not valid UTF-8
- App is responsible for hashing before DB lookup

### `path_prefix(prefix)`

Extracts first segment after prefix, strips prefix + segment from request URI.

- `/t/acme/dashboard` with prefix `"/t"` → `Slug("acme")`, downstream sees `/dashboard`
- Error if path doesn't start with prefix or no segment after it

### `path_param(name)`

Reads named axum path parameter, returns `TenantId::Slug`. No path modification — routes must include the param in their pattern.

- `/{tenant}/dashboard` with param name `"tenant"` → `Slug("acme")`
- Error if param not found (500 — misconfiguration)

## Middleware

### Construction

```rust
let middleware = tenant::middleware(strategy, resolver);
```

Applied to a router group via `.layer()`. All routes in the group require a valid tenant.

### Request flow

1. Strategy extracts `TenantId` from request parts
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
    let org = tenant.get(); // &Org
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
    id.rs           — TenantId enum
    traits.rs       — HasTenantId, TenantResolver traits
    strategy.rs     — TenantStrategy trait + all built-in strategies
    middleware.rs   — TenantLayer, TenantService, tenant::middleware() constructor
    extractor.rs    — Tenant<T> struct + FromRequestParts impl
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

// Strategy constructors
tenant::subdomain("app.com")
tenant::domain()
tenant::subdomain_or_domain("app.com")
tenant::header("X-Tenant-Id")
tenant::api_key_header("X-Api-Key")
tenant::path_prefix("/t")
tenant::path_param("tenant")

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
        match id {
            TenantId::Slug(slug) => {
                sqlx::query_as("SELECT id, name, slug FROM orgs WHERE slug = ?")
                    .bind(slug).fetch_optional(self.db.read_pool()).await?
            }
            TenantId::Domain(domain) => {
                sqlx::query_as("SELECT id, name, slug FROM orgs WHERE custom_domain = ?")
                    .bind(domain).fetch_optional(self.db.read_pool()).await?
            }
            TenantId::ApiKey(key) => {
                let key_hash = sha256(key);
                sqlx::query_as(
                    "SELECT o.id, o.name, o.slug FROM orgs o
                     JOIN api_keys k ON k.tenant_id = o.id WHERE k.key_hash = ?"
                ).bind(key_hash).fetch_optional(self.db.read_pool()).await?
            }
            TenantId::Id(id) => {
                sqlx::query_as("SELECT id, name, slug FROM orgs WHERE id = ?")
                    .bind(id).fetch_optional(self.db.read_pool()).await?
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
- Display/Debug formatting
- Equality comparisons

**`strategy.rs`** — each strategy:
- `subdomain`: valid subdomain extraction, bare base domain (error), missing Host (error), port stripping
- `domain`: valid Host, missing Host (error), port stripping
- `subdomain_or_domain`: subdomain branch, custom domain branch, missing Host (error)
- `header` / `api_key_header`: present header, missing header (error), non-UTF-8 (error)
- `path_prefix`: valid prefix with segment, missing segment (error), wrong prefix (error), URI rewriting (stripped path)
- `path_param`: present param, missing param (error)

**`middleware.rs`** — request flow:
- Strategy succeeds + resolver succeeds → inner service called, tenant in extensions, tracing span has `tenant_id`
- Strategy fails → 400 returned, inner service not called
- Resolver returns not found → 404 propagated, inner service not called
- Resolver returns internal error → 500 propagated

**`extractor.rs`** — `Tenant<T>`:
- Tenant in extensions → extracts successfully
- Tenant not in extensions → 500 error
- `Option<Tenant<T>>` without extensions → `None`
