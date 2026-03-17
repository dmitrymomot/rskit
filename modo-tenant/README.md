# modo-tenant

Multi-tenancy support for modo applications: resolve, cache, and extract the current tenant from any HTTP request signal.

## Features

| Feature | Description |
|---------|-------------|
| `templates` | Enables `TenantContextLayer`, which injects the resolved tenant into the MiniJinja `TemplateContext` under the key `"tenant"`. Requires `modo/templates`. |

## Usage

### Define your tenant type

```rust
use modo_tenant::HasTenantId;

#[derive(Clone, serde::Serialize)]
pub struct MyTenant {
    pub id: String,
    pub slug: String,
    pub name: String,
}

impl HasTenantId for MyTenant {
    fn tenant_id(&self) -> &str {
        &self.id
    }
}
```

### Implement a resolver

```rust
use modo_tenant::{HasTenantId, TenantResolver};
use modo::axum::http::request::Parts;

pub struct DbTenantResolver {
    // e.g., a database connection pool
}

impl TenantResolver for DbTenantResolver {
    type Tenant = MyTenant;

    async fn resolve(
        &self,
        parts: &Parts,
    ) -> Result<Option<MyTenant>, modo::Error> {
        // Extract a signal from `parts` (subdomain, header, path, etc.)
        // and load the tenant from your database.
        // Return Ok(None) when no tenant matches.
        Ok(None)
    }
}
```

### Register with AppBuilder

```rust
use modo_tenant::TenantResolverService;

#[derive(serde::Deserialize, Default)]
struct AppConfig {}

#[modo::main]
async fn main(
    app: modo::app::AppBuilder,
    _config: AppConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let resolver = TenantResolverService::new(DbTenantResolver { /* ... */ });
    app.service(resolver).run().await
}
```

### Extract in handlers

`Tenant<T>` implements `Deref<Target = T>`, so tenant fields are accessible directly. `OptionalTenant<T>` implements `Deref<Target = Option<T>>`.

```rust
use modo_tenant::{Tenant, OptionalTenant};

// Requires a tenant — returns HTTP 404 if none is resolved.
async fn dashboard(tenant: Tenant<MyTenant>) {
    println!("tenant: {}", tenant.name);
}

// Optional — never rejects on missing tenant; inner value is None when no
// tenant matches. Returns HTTP 500 if the resolver fails or is not registered.
async fn home(tenant: OptionalTenant<MyTenant>) {
    if let Some(t) = &*tenant {
        println!("tenant: {}", t.name);
    }
}
```

### Built-in resolvers

#### Subdomain

```rust
use modo_tenant::{SubdomainResolver, TenantResolverService};

let resolver = SubdomainResolver::new("myapp.com", |slug| async move {
    // load tenant by slug from DB; return Ok(None) when not found
    Ok::<Option<MyTenant>, modo::Error>(None)
});
let svc = TenantResolverService::new(resolver);
```

`acme.myapp.com` → slug `"acme"`. The bare domain and reserved subdomains
(`"www"`, `"api"`, `"admin"`, `"mail"`) return `None`. Port suffixes are
stripped automatically. Use `SubdomainResolver::with_reserved` to override
the reserved list.

#### HTTP header

```rust
use modo_tenant::{HeaderResolver, TenantResolverService};

let resolver = HeaderResolver::new("x-tenant-id", |id| async move {
    // load tenant by id from DB; return Ok(None) when not found
    Ok::<Option<MyTenant>, modo::Error>(None)
});
let svc = TenantResolverService::new(resolver);
```

The header value is trimmed of whitespace. Missing or whitespace-only headers return `None`.

> **Security:** The header value is fully controlled by the client. Without a
> reverse proxy that strips or overwrites the configured header, any client can
> impersonate any tenant. Use `HeaderResolver` only behind a trusted reverse
> proxy that sets the header, or for internal/API-only services with
> authenticated callers.

#### Path prefix

```rust
use modo_tenant::{PathPrefixResolver, TenantResolverService};

let resolver = PathPrefixResolver::new(|slug| async move {
    // load tenant by slug from DB; return Ok(None) when not found
    Ok::<Option<MyTenant>, modo::Error>(None)
});
let svc = TenantResolverService::new(resolver);
```

`/acme/dashboard` → slug `"acme"`. The root path `/` returns `None`.

### Template context injection (feature `templates`)

```rust
use modo_tenant::{TenantContextLayer, TenantResolverService};

#[derive(serde::Deserialize, Default)]
struct AppConfig {}

#[modo::main]
async fn main(
    app: modo::app::AppBuilder,
    _config: AppConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let svc = TenantResolverService::new(DbTenantResolver { /* ... */ });
    let layer = TenantContextLayer::new(svc);
    app.layer(layer).run().await
}
```

Inside templates the tenant is accessible as `{{ tenant.name }}`. Resolution errors are logged at `WARN` level and the request continues without a tenant in context (fail-open). If your application requires tenant context for security-sensitive rendering, use the `Tenant<T>` extractor instead — it returns HTTP 500 on resolver errors.

## Key Types

| Type | Description |
|------|-------------|
| `HasTenantId` | Trait a tenant type must implement to expose its unique ID. |
| `TenantResolver` | Trait for pluggable tenant resolution strategies. |
| `TenantResolverService<T>` | Type-erased, cheaply cloneable wrapper registered via `AppBuilder::service()`. |
| `Tenant<T>` | Extractor that requires a tenant; returns HTTP 404 when absent, HTTP 500 on resolver error. |
| `OptionalTenant<T>` | Extractor that yields `Option<T>`; never rejects on missing tenant. |
| `SubdomainResolver<T, F>` | Resolves tenant from the subdomain of the `Host` header. |
| `HeaderResolver<T, F>` | Resolves tenant from a named HTTP header. |
| `PathPrefixResolver<T, F>` | Resolves tenant from the first URL path segment. |
| `TenantContextLayer<T>` | Tower layer that injects the tenant into `TemplateContext` (feature `templates`). |

## Caching

The resolved tenant is cached in request extensions after the first lookup. When `Tenant<T>` and `OptionalTenant<T>` are both declared as handler parameters — or when `TenantContextLayer` runs before an extractor — the underlying resolver is called only once per request.
