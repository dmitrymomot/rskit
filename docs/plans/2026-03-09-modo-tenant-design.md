# modo-tenant Design

Multi-tenancy and role-based access control for modo.

## Crates

- `modo-tenant` — traits, extractors, built-in resolvers, context layer, guard functions
- `modo-tenant-macros` — `#[allow_roles()]` / `#[deny_roles()]` proc macros
- `modo-auth` (update) — add `UserContextLayer<U>` for template injection

## Traits

### `HasTenantId`

User's tenant type must expose its ID.

```rust
pub trait HasTenantId {
    fn tenant_id(&self) -> &str;
}
```

### `TenantResolver`

Resolves a tenant from HTTP request parts. Pluggable strategy — user picks subdomain, header, path prefix, or custom logic.

```rust
pub trait TenantResolver: Send + Sync + 'static {
    type Tenant: Clone + Send + Sync + HasTenantId + Serialize + 'static;

    fn resolve(&self, parts: &Parts)
        -> impl Future<Output = Result<Option<Self::Tenant>, Error>> + Send;
}
```

Follows the `UserProvider` pattern: object-safe bridge trait internally, public `TenantResolverService<T>` wrapper for service registry registration.

### `MemberProvider`

Loads membership records and tenant lists for a user.

```rust
pub trait MemberProvider: Send + Sync + 'static {
    type Member: Clone + Send + Sync + Serialize + 'static;
    type Tenant: Clone + Send + Sync + HasTenantId + Serialize + 'static;

    fn find_member(&self, user_id: &str, tenant_id: &str)
        -> impl Future<Output = Result<Option<Self::Member>, Error>> + Send;

    fn list_tenants(&self, user_id: &str)
        -> impl Future<Output = Result<Vec<Self::Tenant>, Error>> + Send;

    fn role(&self, member: &Self::Member) -> &str;
}
```

Wrapped in `MemberProviderService<M>` for registration. Roles are flat strings.

## Extractors

### `Tenant<T>` — required tenant

Resolves tenant from request. Returns 404 if no tenant found.

```rust
pub struct Tenant<T>(pub T);
```

Flow:
1. Check extensions for cached `ResolvedTenant<T>` — return if found
2. Get `TenantResolverService<T>` from service registry
3. Call `resolve(parts)` — `None` = 404, `Err` = 500
4. Cache `ResolvedTenant<T>` in extensions
5. Return `Tenant(t)`

### `OptionalTenant<T>` — optional tenant

Same as `Tenant<T>` but returns `None` instead of 404.

```rust
pub struct OptionalTenant<T>(pub Option<T>);
```

### `Member<T, M>` — tenant + auth + membership

Self-contained extractor with two type parameters. Returns 404 (no tenant), 401 (no auth), or 403 (not a member).

```rust
pub struct Member<T: HasTenantId, M> {
    tenant: T,
    inner: M,
    role: String,
}

impl<T: HasTenantId, M> Member<T, M> {
    pub fn tenant(&self) -> &T { &self.tenant }
    pub fn role(&self) -> &str { &self.role }
    pub fn into_inner(self) -> M { self.inner }
}

impl<T: HasTenantId, M> Deref for Member<T, M> {
    type Target = M;
    fn deref(&self) -> &M { &self.inner }
}
```

Flow:
1. Resolve `Tenant<T>` (cached or fresh)
2. Get `user_id` from session — missing = 401
3. Get `MemberProviderService<M>` from service registry
4. Call `find_member(user_id, tenant.tenant_id())` — `None` = 403
5. Call `role(&member)`, store as owned `String`
6. Cache `ResolvedMember<M>`, `ResolvedRole` in extensions

### `TenantContext<T, M, U>` — full context for handler logic

Combines tenant, member, user, and tenants list. For handlers that need all data in code (not just templates).

```rust
pub struct TenantContext<T: HasTenantId, M, U> {
    tenant: T,
    member: M,
    user: U,
    tenants: Vec<T>,
    role: String,
}
```

## Template Context Layers

### `UserContextLayer<U>` (in `modo-auth`)

Injects `user` into `TemplateContext`. If no authenticated user, injects null. Works without tenancy.

### `TenantContextLayer<T, M>` (in `modo-tenant`)

Injects `tenant`, `member`, `tenants`, `role` into `TemplateContext`. Reads cached user from extensions (set by `UserContextLayer`). If no tenant or no auth, injects nulls/empty — public pages still work.

### Layer ordering

```
... → Session → UserContextLayer → TenantContextLayer → ...
```

Both layers auto-registered when their services + `TemplateEngine` are present.

## Built-in Resolvers

Three ready-to-use `TenantResolver` implementations. Each extracts an identifier string from the request, then delegates to a user-provided async lookup function.

### `SubdomainResolver`

Parses subdomain from `Host` header.

```rust
let resolver = SubdomainResolver::new("myapp.com", |slug| async {
    db.find_tenant_by_slug(&slug).await
});
// acme.myapp.com → identifier = "acme"
```

### `HeaderResolver`

Reads a custom header value.

```rust
let resolver = HeaderResolver::new("X-Tenant-ID", |id| async {
    db.find_tenant_by_id(&id).await
});
```

### `PathPrefixResolver`

Strips the first path segment as identifier. Rewrites URI for downstream handlers.

```rust
let resolver = PathPrefixResolver::new(|slug| async {
    db.find_tenant_by_slug(&slug).await
});
// /acme/dashboard → identifier = "acme", path rewritten to /dashboard
```

All three share the same generic shape:

```rust
pub struct SubdomainResolver<T, F> { ... }

impl<T, F, Fut> TenantResolver for SubdomainResolver<T, F>
where
    T: Clone + Send + Sync + HasTenantId + Serialize + 'static,
    F: Fn(String) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<Option<T>, Error>> + Send,
{ ... }
```

## Role Guard Macros

Attribute macros in `modo-tenant-macros`.

```rust
#[allow_roles("admin", "owner")]  // only these roles
#[deny_roles("viewer")]           // block these roles
```

### Expansion

```rust
// Written:
#[modo::handler(GET, "/admin")]
#[allow_roles("admin")]
async fn admin_page() -> Result<&'static str, Error> {
    Ok("admin")
}

// Expands to:
#[modo::handler(GET, "/admin")]
#[middleware(modo_tenant::require_roles("admin"))]
async fn admin_page() -> Result<&'static str, Error> {
    Ok("admin")
}
```

No `Member` extractor required in the handler signature.

### `RoleResolver`

Type-erased role resolver registered in the service registry. Created automatically when `TenantResolverService<T>` and `MemberProviderService<M>` are both registered. Captures generic types at registration time.

Resolves: request parts → tenant → user_id from session → member → role string.

The `require_roles` / `exclude_roles` middleware functions read `RoleResolver` from the service registry (or from cached `ResolvedRole` in extensions if already resolved).

## Per-Request Caching

All resolution results cached in request extensions:

```rust
struct ResolvedTenant<T>(Arc<T>);
struct ResolvedUser<U>(Arc<U>);
struct ResolvedMember<M>(Arc<M>);
struct ResolvedRole(String);
struct ResolvedTenants<T>(Arc<Vec<T>>);
```

Layers, middleware, and extractors all read/write the same cache. Each entity resolved at most once per request regardless of how many consumers need it.

## Error Handling

All errors use `modo::HttpError` / `modo::Error` from core. No custom error types.

| Condition | Error |
|-----------|-------|
| Tenant not resolved | `HttpError::NotFound` |
| Service not registered | `Error::internal(...)` |
| No authenticated user | `HttpError::Unauthorized` |
| Not a member of tenant | `HttpError::Forbidden` |
| Role not allowed | `HttpError::Forbidden` |

## Registration

```rust
#[modo::main]
async fn main(app: AppBuilder, config: AppConfig) -> Result<...> {
    let db = modo_db::connect(&config.database).await?;
    let resolver = SubdomainResolver::new("myapp.com", |slug| async { ... });
    let member_provider = MyMemberProvider::new(db.clone());
    let user_provider = MyUserProvider::new(db.clone());

    app.server_config(config.server)
        .managed_service(db)
        .service(TenantResolverService::new(resolver))
        .service(MemberProviderService::new(member_provider))
        .service(UserProviderService::new(user_provider))
        .run()
        .await
}
```

Context layers auto-registered when services + `TemplateEngine` are present.

## Dependencies

- `modo-tenant` depends on: `modo` (core), `modo-session` (user_id from session), `serde` (Serialize bound)
- `modo-tenant-macros` depends on: `syn`, `quote`, `proc-macro2`
- `modo-auth` update: no new dependencies (already has `modo`, `modo-session`, `serde`)
- No dependency on `modo-db` — query scoping is the application's responsibility
