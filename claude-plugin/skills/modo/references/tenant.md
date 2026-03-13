# Tenant Reference

The `modo-tenant` crate provides multi-tenancy support: tenant resolution from HTTP requests via
configurable strategies, extractors for use in handlers, and optional template context injection.
Tenant resolution is pluggable — you supply the lookup logic, and the crate wires it into the
request lifecycle. There is no automatic database filtering; tenant-scoped queries require manual
WHERE clauses.

---

## Documentation

- modo-tenant crate: https://docs.rs/modo-tenant

---

## Tenant Resolution

### `HasTenantId` trait

Every tenant type must implement `HasTenantId` to expose its unique identifier:

```rust
pub trait HasTenantId {
    fn tenant_id(&self) -> &str;
}
```

The `Tenant` and `OptionalTenant` extractors and `TenantContextLayer` all require the inner type
to implement this trait, plus `Clone + Send + Sync + serde::Serialize + 'static`.

### `TenantResolver` trait

`TenantResolver` is the core pluggable strategy trait. Implement it to teach the framework how to
identify a tenant from any signal in the request:

```rust
pub trait TenantResolver: Send + Sync + 'static {
    type Tenant: Clone + Send + Sync + HasTenantId + serde::Serialize + 'static;

    fn resolve(
        &self,
        parts: &Parts,
    ) -> impl Future<Output = Result<Option<Self::Tenant>, modo::Error>> + Send;
}
```

Return semantics:
- `Ok(Some(tenant))` — tenant identified successfully.
- `Ok(None)` — no tenant found (e.g. a public route). The `Tenant` extractor returns 404.
- `Err(e)` — infrastructure failure (e.g. database error). Both `Tenant` and `OptionalTenant`
  return 500.

Example custom resolver:

```rust
use modo_tenant::{HasTenantId, TenantResolver};
use modo::axum::http::request::Parts;

#[derive(Clone, serde::Serialize)]
pub struct Workspace {
    pub id: String,
    pub slug: String,
    pub display_name: String,
}

impl HasTenantId for Workspace {
    fn tenant_id(&self) -> &str {
        &self.id
    }
}

pub struct WorkspaceResolver {
    db: DatabaseConnection,
}

impl TenantResolver for WorkspaceResolver {
    type Tenant = Workspace;

    async fn resolve(&self, parts: &Parts) -> Result<Option<Self::Tenant>, modo::Error> {
        let host = match parts.headers.get("host").and_then(|v| v.to_str().ok()) {
            Some(h) => h.split(':').next().unwrap_or(h),
            None => return Ok(None),
        };
        let slug = host.strip_suffix(".myapp.com").unwrap_or("");
        if slug.is_empty() || slug == "www" {
            return Ok(None);
        }
        // Query your database for the workspace
        let ws = workspace::Entity::find()
            .filter(workspace::Column::Slug.eq(slug))
            .one(&self.db)
            .await
            .map_err(modo::Error::internal)?;
        Ok(ws.map(|w| Workspace { id: w.id, slug: w.slug, display_name: w.name }))
    }
}
```

### Built-in Resolution Strategies

Three built-in resolvers cover the most common strategies. All three take a `lookup` closure that
receives the extracted identifier string and returns the tenant asynchronously.

#### `SubdomainResolver`

Extracts the tenant slug from the subdomain of the `Host` header. Given `base_domain = "myapp.com"`,
a request to `acme.myapp.com` passes `"acme"` to the lookup closure. The bare domain and the
`www` subdomain always return `Ok(None)`. Port suffixes are stripped before matching.

```rust
use modo_tenant::SubdomainResolver;

let resolver = SubdomainResolver::new("myapp.com", |slug| async move {
    // slug = "acme" for requests to acme.myapp.com
    let ws = db.find_workspace_by_slug(&slug).await?;
    Ok(ws)
});
```

Multi-level subdomains (e.g. `a.b.myapp.com`) are supported — the entire prefix before the base
domain (`"a.b"`) is passed to the lookup closure.

#### `HeaderResolver`

Extracts the tenant identifier from a named HTTP header. The header value is trimmed of surrounding
whitespace before being forwarded. Returns `Ok(None)` when the header is absent or contains only
whitespace.

```rust
use modo_tenant::HeaderResolver;

let resolver = HeaderResolver::new("x-tenant-id", |id| async move {
    // id = trimmed header value
    let ws = db.find_workspace_by_id(&id).await?;
    Ok(ws)
});
```

Useful for internal or API-only services where the caller controls the headers.

#### `PathPrefixResolver`

Extracts the tenant identifier from the first path segment of the request URI. A request to
`/acme/dashboard` passes `"acme"` to the lookup closure. The root path `/` returns `Ok(None)`.

```rust
use modo_tenant::PathPrefixResolver;

let resolver = PathPrefixResolver::new(|slug| async move {
    // slug = "acme" for /acme/dashboard
    // Return Ok(None) quickly for non-tenant segments like "assets", "api"
    if slug == "assets" || slug == "api" {
        return Ok(None);
    }
    let ws = db.find_workspace_by_slug(&slug).await?;
    Ok(ws)
});
```

The lookup closure is called for every request's first segment, including static asset paths and
API routes — the closure is responsible for returning `Ok(None)` quickly for non-tenant slugs.

---

## Type-Erased Service

### `TenantResolverService<T>`

`TenantResolverService<T>` is a cheaply cloneable, type-erased wrapper around any
`TenantResolver`. Register one instance with `AppState`'s service registry, and all extractors
and middleware can retrieve it at request time without knowing the concrete resolver type.

```rust
pub struct TenantResolverService<T: Clone + Send + Sync + 'static> {
    inner: Arc<dyn TenantResolverDyn<T>>,
}

impl<T: Clone + Send + Sync + 'static> TenantResolverService<T> {
    pub fn new<R: TenantResolver<Tenant = T>>(resolver: R) -> Self { ... }
    pub async fn resolve(&self, parts: &Parts) -> Result<Option<T>, modo::Error> { ... }
}
```

The type erasure happens via an internal `TenantResolverDyn<T>` bridge trait (not public) that
boxes the future returned by `resolve`. The `Arc` wrapper makes `TenantResolverService<T>` cheap
to clone across middleware layers.

**Registration:**

```rust
use modo_tenant::{SubdomainResolver, TenantResolverService};

#[modo::main]
async fn main(
    app: modo::app::AppBuilder,
    config: Config,
) -> Result<(), Box<dyn std::error::Error>> {
    let resolver = SubdomainResolver::new("myapp.com", {
        let db = config.db.clone();
        move |slug| {
            let db = db.clone();
            async move { db.find_workspace_by_slug(&slug).await }
        }
    });
    let tenant_svc = TenantResolverService::new(resolver);

    app.config(config.core)
        .service(tenant_svc)
        .run()
        .await
}
```

The `AppState::services` registry is keyed by type: `TenantResolverService<Workspace>` and
`TenantResolverService<Account>` are distinct entries, so multiple tenant types are supported
simultaneously.

### Resolution Caching

The resolved tenant is cached in the request extensions under `ResolvedTenant<T>` (an internal
type wrapping `Arc<T>`). Both the `Tenant<T>` and `OptionalTenant<T>` extractors, as well as
`TenantContextLayer`, call `resolve_and_cache` which checks this cache first. When a handler
uses both extractors (e.g. `Tenant<Workspace>` and `OptionalTenant<Workspace>`), the underlying
resolver is only called once per request.

---

## Extractors

### `Tenant<T>`

Requires a resolved tenant. If the resolver returns `Ok(None)`, the request is rejected with
HTTP 404. If the resolver returns `Err(...)`, the request is rejected with HTTP 500.

```rust
#[derive(Clone)]
pub struct Tenant<T: Clone + Send + Sync + 'static>(pub T);
```

`Tenant<T>` implements `Deref<Target = T>`, so all methods of `T` are directly accessible:

```rust
use modo_tenant::Tenant;

#[modo::handler(GET, "/dashboard")]
async fn dashboard(tenant: Tenant<Workspace>) -> HandlerResult<Json<serde_json::Value>> {
    // Access fields via deref
    let name = &tenant.display_name;
    // Or use .0 for the inner value
    let id = &tenant.0.id;
    Ok(modo::Json(serde_json::json!({ "workspace": name })))
}
```

Use `Tenant<T>` on routes that must always have a tenant — it guards access at the framework
level before any handler code runs.

### `OptionalTenant<T>`

Resolves a tenant when present, but never rejects the request due to a missing tenant. The inner
`Option<T>` is `None` when the resolver returns `Ok(None)`.

```rust
#[derive(Clone)]
pub struct OptionalTenant<T: Clone + Send + Sync + 'static>(pub Option<T>);
```

`OptionalTenant<T>` implements `Deref<Target = Option<T>>`, providing direct access to
`Option` methods.

**Important distinction:** `OptionalTenant` only suppresses the "no tenant found" case. If the
resolver itself fails (e.g. a database error), the extractor still rejects with 500. This
differs from `TenantContextLayer`, which silently swallows resolver errors (logging at WARN level)
to avoid disrupting the request.

```rust
use modo_tenant::OptionalTenant;

#[modo::handler(GET, "/")]
async fn home(tenant: OptionalTenant<Workspace>) -> HandlerResult<String> {
    match tenant.0 {
        Some(ws) => Ok(format!("Welcome to {}", ws.display_name)),
        None => Ok("Welcome".to_string()),
    }
}
```

Use `OptionalTenant<T>` on routes that serve both tenant-specific and public audiences (e.g.
a landing page that is personalized when a tenant subdomain is detected).

---

## Integration Patterns

### Tenant in Templates (`TenantContextLayer`)

The `TenantContextLayer` (feature `"templates"`) is a Tower middleware layer that injects the
resolved tenant into the request's `TemplateContext` under the key `"tenant"`. This makes tenant
data available to all Jinja/MiniJinja templates rendered during the request without requiring
the handler to pass it explicitly.

```rust
#[cfg(feature = "templates")]
pub struct TenantContextLayer<T>
where
    T: Clone + Send + Sync + HasTenantId + serde::Serialize + 'static;
```

Behaviour:
- If the tenant resolves successfully, it is serialized and inserted as `ctx["tenant"]`.
- If resolution fails, the error is logged at WARN level and the request continues without
  `"tenant"` in the context (the layer never returns an error response).
- If no `TemplateContext` extension is present in the request, the layer is a no-op for context
  injection but still passes the request through normally.

**Registration:**

```rust
use modo_tenant::{TenantContextLayer, TenantResolverService};

let tenant_svc = TenantResolverService::new(resolver);
let tenant_layer = TenantContextLayer::new(tenant_svc.clone());

// Apply as a global or module-level middleware layer
app.layer(tenant_layer)
```

**In templates:**

```jinja
{% if tenant %}
  <h1>{{ tenant.display_name }}</h1>
{% else %}
  <h1>Welcome</h1>
{% endif %}
```

The tenant type must implement `serde::Serialize` — all public fields are available in templates
by their Rust field names (or `#[serde(rename = "...")]` overrides).

`TenantContextLayer` uses the same `resolve_and_cache` function as the extractors, so when a
handler also uses `Tenant<T>` or `OptionalTenant<T>` in the same request, the resolver is still
only called once.

### Tenant-Scoped Database Queries

`modo-tenant` does not automatically filter database queries by tenant. Every query that should
be scoped to the current tenant must include an explicit WHERE clause. Retrieve the tenant from
the `Tenant<T>` extractor and pass its ID to the query:

```rust
use modo_tenant::Tenant;
use sea_orm::EntityTrait;

#[modo::handler(GET, "/projects")]
async fn list_projects(
    db: Service<DatabaseConnection>,
    tenant: Tenant<Workspace>,
) -> JsonResult<Vec<ProjectResponse>> {
    let projects = project::Entity::find()
        .filter(project::Column::WorkspaceId.eq(tenant.tenant_id()))
        .all(&*db)
        .await?;

    Ok(modo::Json(projects.into_iter().map(Into::into).collect()))
}
```

For deeply nested resources where the tenant ID is not on the direct table, join through the
parent chain:

```rust
// Comment belongs to Post belongs to Workspace
comment::Entity::find()
    .inner_join(post::Entity)
    .filter(post::Column::WorkspaceId.eq(tenant.tenant_id()))
    .filter(comment::Column::PostId.eq(post_id))
    .all(&*db)
    .await?
```

There is no row-level security or automatic tenant filter — forgetting the WHERE clause leaks
cross-tenant data. Consider a helper function or repository layer that always takes a
`&str` tenant ID to avoid accidental omission.

### Multiple Tenant Types

A single application can support multiple tenant types simultaneously by registering multiple
`TenantResolverService` instances (one per tenant type) with the service registry. Each is keyed
by its type parameter, so they do not conflict:

```rust
let workspace_svc = TenantResolverService::new(WorkspaceResolver::new(db.clone()));
let account_svc = TenantResolverService::new(AccountResolver::new(db.clone()));

app.service(workspace_svc)
   .service(account_svc)
```

Handlers can then extract `Tenant<Workspace>` and `Tenant<Account>` independently on different
route sets.

### Middleware Stacking with Tenant Resolution

`TenantContextLayer` is a Tower `Layer`, so stacking order follows the standard modo convention:
Global (outermost) → Module → Handler (innermost). Apply `TenantContextLayer` at the global level
so tenant data is available in templates regardless of which route is matched.

```rust
use modo_tenant::{TenantContextLayer, TenantResolverService};

let tenant_svc = TenantResolverService::new(SubdomainResolver::new(
    "myapp.com",
    |slug| async move { db.find_workspace_by_slug(&slug).await },
));

// Register the service for extractor use
app.service(tenant_svc.clone())
   // Apply context layer globally so templates always have "tenant"
   .layer(TenantContextLayer::new(tenant_svc))
```

When a route module also uses `Tenant<T>` extractors, caching ensures the resolver is only called
once per request even though both the layer and the extractor participate in resolution.

### Tenant Verification (Access Control)

`TenantResolver::resolve` identifies the tenant from the request signal, but does not enforce
ownership. After extracting a `Tenant<T>`, verify that the requested resource belongs to that
tenant before returning it:

```rust
#[modo::handler(GET, "/projects/:project_id")]
async fn get_project(
    db: Service<DatabaseConnection>,
    tenant: Tenant<Workspace>,
    modo::axum::extract::Path(project_id): modo::axum::extract::Path<String>,
) -> JsonResult<ProjectResponse> {
    let project = project::Entity::find_by_id(&project_id)
        .filter(project::Column::WorkspaceId.eq(tenant.tenant_id()))
        .one(&*db)
        .await?
        .ok_or(modo::Error::from(modo::HttpError::NotFound))?;

    Ok(modo::Json(project.into()))
}
```

The combined `find_by_id` + `workspace_id` filter ensures the project both exists and belongs to
the current tenant. A project in a different workspace returns 404 rather than leaking its
existence.

### Custom Resolver with Connection Pool

When the resolver needs a database connection, capture the pool at construction time using a
closure. The closure must be `Send + Sync + 'static`, so clone the pool before moving it in:

```rust
use sea_orm::DatabaseConnection;
use modo_tenant::{SubdomainResolver, TenantResolverService};

fn workspace_resolver(db: DatabaseConnection) -> TenantResolverService<Workspace> {
    let resolver = SubdomainResolver::new("myapp.com", move |slug| {
        let db = db.clone();  // DatabaseConnection is Arc-backed, cheap to clone
        async move {
            let row = workspace::Entity::find()
                .filter(workspace::Column::Slug.eq(&slug))
                .filter(workspace::Column::ActiveAt.is_not_null())
                .one(&db)
                .await
                .map_err(|e| modo::Error::internal(e.to_string()))?;
            Ok(row.map(Into::into))
        }
    });
    TenantResolverService::new(resolver)
}
```

`DatabaseConnection` from SeaORM is internally reference-counted, so cloning it inside the async
block is inexpensive and safe across threads.

---

## Gotchas

- **Missing service registration returns 500.** If `TenantResolverService<T>` is not registered
  with `AppState`, both `Tenant<T>` and `OptionalTenant<T>` reject with an internal server error,
  not a 404. Always register the service in `main`.

- **`OptionalTenant` still fails on resolver errors.** Unlike `TenantContextLayer`, which swallows
  resolver errors, `OptionalTenant` propagates them as 500. `Ok(None)` is the only case that
  produces `None` inside the extractor.

- **`TenantContextLayer` requires the `"templates"` feature.** The type is gated behind
  `#[cfg(feature = "templates")]`. Add `modo-tenant = { features = ["templates"] }` to your
  `Cargo.toml`. The `templates` feature activates `modo/templates` transitively.

- **`PathPrefixResolver` calls lookup for every request.** The lookup closure is invoked for
  every first path segment including `"assets"`, `"favicon.ico"`, `"api"`, etc. Return `Ok(None)`
  immediately for non-tenant slugs to avoid spurious database queries.

- **Subdomain resolver skips `www`.** `SubdomainResolver` explicitly returns `Ok(None)` for the
  `www` subdomain and the bare domain. If you need custom handling for `www.myapp.com`, implement
  `TenantResolver` directly.

- **Header value is trimmed, not validated.** `HeaderResolver` trims whitespace but performs no
  further sanitization. If the header value is user-controlled (e.g. forwarded from a proxy), the
  lookup closure must validate it against your database before trusting it as a tenant identifier.

- **Resolution caching is per-request.** The `ResolvedTenant<T>` cached in request extensions
  lives only for the lifetime of a single request. There is no cross-request cache — each request
  calls the resolver (or hits the per-request cache if multiple extractors are used).

- **Resolver must be `Send + Sync + 'static`.** Database connection types such as SeaORM's
  `DatabaseConnection` satisfy these bounds. If your resolver holds non-`Send` state, wrap it in
  an appropriate synchronization primitive.

- **No automatic tenant injection into jobs.** Background jobs (`modo_jobs`) run outside the HTTP
  request lifecycle. Pass the tenant ID explicitly as part of the job payload if the job needs to
  operate on behalf of a tenant.

- **`TenantContextLayer` is a no-op without `TemplateContext`.** If no `TemplateContext` extension
  is present in the request extensions, the layer does not panic or error — it simply skips
  context injection and forwards the request as-is. The `TemplateEngine` service inserts
  `TemplateContext` automatically; if you are using `TenantContextLayer` without a template engine
  the tenant will never appear in templates.

---

## docs.rs Links

| Type / Item                    | URL                                                                               |
|--------------------------------|-----------------------------------------------------------------------------------|
| `HasTenantId` trait            | https://docs.rs/modo-tenant/latest/modo_tenant/trait.HasTenantId.html             |
| `TenantResolver` trait         | https://docs.rs/modo-tenant/latest/modo_tenant/trait.TenantResolver.html          |
| `TenantResolverService<T>`     | https://docs.rs/modo-tenant/latest/modo_tenant/struct.TenantResolverService.html  |
| `Tenant<T>` extractor          | https://docs.rs/modo-tenant/latest/modo_tenant/struct.Tenant.html                 |
| `OptionalTenant<T>` extractor  | https://docs.rs/modo-tenant/latest/modo_tenant/struct.OptionalTenant.html         |
| `TenantContextLayer`           | https://docs.rs/modo-tenant/latest/modo_tenant/struct.TenantContextLayer.html     |
| `SubdomainResolver`            | https://docs.rs/modo-tenant/latest/modo_tenant/struct.SubdomainResolver.html      |
| `HeaderResolver`               | https://docs.rs/modo-tenant/latest/modo_tenant/struct.HeaderResolver.html         |
| `PathPrefixResolver`           | https://docs.rs/modo-tenant/latest/modo_tenant/struct.PathPrefixResolver.html     |
