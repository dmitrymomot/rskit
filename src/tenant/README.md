# modo::tenant

Multi-tenant request routing for the `modo` web framework.

The module resolves a tenant from every incoming HTTP request using a
two-step pipeline: a **strategy** extracts a raw identifier, and a
**resolver** maps that identifier to an app-defined tenant type. The
resolved tenant is stored in request extensions and surfaced to handlers
via the `Tenant<T>` axum extractor.

## Key Types

| Item                             | Kind           | Purpose                                                                      |
| -------------------------------- | -------------- | ---------------------------------------------------------------------------- |
| `TenantId`                       | enum           | Raw identifier extracted from the request (`Slug`, `Domain`, `Id`, `ApiKey`) |
| `TenantStrategy`                 | trait          | Extracts a `TenantId` from `http::request::Parts`                            |
| `TenantResolver`                 | trait          | Maps a `TenantId` to the app's concrete tenant type (RPITIT; not object-safe) |
| `HasTenantId`                    | trait          | Required bound on the resolved tenant; provides the tracing field value      |
| `Tenant<T>`                      | extractor      | Retrieves the resolved tenant from request extensions in handlers            |
| `TenantLayer<S, R>`              | Tower layer    | Middleware layer produced by `middleware()`                                  |
| `TenantMiddleware<Svc, S, R>`    | Tower service  | The inner service wrapping each request                                      |
| `middleware(strategy, resolver)` | constructor fn | Primary entry point — builds the `TenantLayer`                               |

## Strategies

| Constructor                       | Struct                      | `TenantId` variant | Source                                  |
| --------------------------------- | --------------------------- | ------------------ | --------------------------------------- |
| `subdomain("base.com")`           | `SubdomainStrategy`         | `Slug`             | Single-level subdomain                  |
| `domain()`                        | `DomainStrategy`            | `Domain`           | Full `Host` header value                |
| `subdomain_or_domain("base.com")` | `SubdomainOrDomainStrategy` | `Slug` or `Domain` | Subdomain or custom domain              |
| `header("x-tenant-id")`           | `HeaderStrategy`            | `Id`               | Named request header                    |
| `api_key_header("x-api-key")`     | `ApiKeyHeaderStrategy`      | `ApiKey`           | Named API key header (redacted in logs) |
| `path_prefix("/org")`             | `PathPrefixStrategy`        | `Slug`             | Path segment; rewrites URI              |
| `path_param("tenant")`            | `PathParamStrategy`         | `Slug`             | Axum path parameter                     |

## Usage

### Define the tenant type

```rust
use modo::tenant::HasTenantId;

#[derive(Clone)]
struct MyTenant {
    id: String,
    name: String,
}

impl HasTenantId for MyTenant {
    fn tenant_id(&self) -> &str {
        &self.id
    }
}
```

### Implement a resolver

```rust
use modo::tenant::{HasTenantId, TenantId, TenantResolver};
use modo::Result;

struct MyResolver;

impl TenantResolver for MyResolver {
    type Tenant = MyTenant;

    async fn resolve(&self, id: &TenantId) -> Result<MyTenant> {
        // Look up tenant in database or cache
        Ok(MyTenant {
            id: id.as_str().to_string(),
            name: "Acme Corp".to_string(),
        })
    }
}
```

### Wire into the router

```rust
use axum::{Router, routing::get};
use modo::tenant::{Tenant, middleware, subdomain};

async fn dashboard(tenant: Tenant<MyTenant>) -> String {
    format!("Hello, {}!", tenant.name)
}

let app = Router::new()
    .route("/dashboard", get(dashboard))
    .layer(middleware(subdomain("example.com"), MyResolver));
```

### Use `Option<Tenant<T>>` for optional tenant routes

```rust
use axum::routing::get;
use modo::tenant::Tenant;

async fn public_or_tenant(tenant: Option<Tenant<MyTenant>>) -> String {
    match tenant {
        Some(t) => format!("Tenant: {}", t.name),
        None => "Public route".to_string(),
    }
}
```

### Path parameter strategy

`PathParamStrategy` reads from an axum route parameter and must be applied
with `.route_layer()` (not `.layer()`), because path parameters are only
available after route matching.

```rust
use axum::{Router, routing::get};
use modo::tenant::{Tenant, middleware, path_param};

let app = Router::new()
    .route("/{tenant}/settings", get(dashboard))
    .route_layer(middleware(path_param("tenant"), MyResolver));
```

### Path prefix strategy

`PathPrefixStrategy` strips the prefix and tenant slug from the URI before
the request reaches handlers, so routes do not need to include the tenant
segment.

```rust
use axum::{Router, routing::get};
use modo::tenant::{Tenant, middleware, path_prefix};

// Incoming: GET /org/acme/settings
// URI seen by handler: GET /settings
let app = Router::new()
    .route("/settings", get(dashboard))
    .layer(middleware(path_prefix("/org"), MyResolver));
```

## `TenantId` and logging

`TenantId::ApiKey` is always redacted in both `Display` and `Debug` output
to prevent accidental logging of secrets. All other variants display their
raw value with a type prefix (`slug:`, `domain:`, `id:`).

`TenantId::as_str()` returns the inner value across all variants, including
the raw API key value — use it only for resolver logic, not for logging.

## Tracing

The middleware calls `Span::current().record("tenant_id", ...)` after a
successful resolve. The `tenant_id` field appears automatically in logs when
using `modo::middleware::tracing()`, which creates `http_request` spans with
`tenant_id = tracing::field::Empty` pre-declared via `ModoMakeSpan`.

For custom spans (e.g., background tasks or `#[tracing::instrument]`), you
must declare the field explicitly:

```rust
#[tracing::instrument(fields(tenant_id = tracing::field::Empty))]
async fn my_handler() { /* ... */ }
```

Spans that do not declare `tenant_id = tracing::field::Empty` silently
ignore the `record()` call.

## Domain management (`domain` submodule)

The `domain` submodule provides `DomainService` for registering, verifying,
and managing custom domains per tenant. Verification uses DNS TXT records
via `modo::dns::DomainVerifier`. A domain must be verified within 48 hours
of registration or it is marked as failed.

### Domain types

| Item                    | Kind       | Purpose                                                       |
| ----------------------- | ---------- | ------------------------------------------------------------- |
| `DomainService`         | struct     | Service for managing domain claims and verification           |
| `DomainClaim`           | struct     | A registered domain claim for a tenant                        |
| `ClaimStatus`           | enum       | Verification status: `Pending`, `Verified`, `Failed`          |
| `TenantMatch`           | struct     | Result of a domain-to-tenant lookup (`tenant_id` + `domain`)  |
| `validate_domain()`     | fn         | Validate and normalize a domain name                          |
| `extract_email_domain()` | fn        | Extract and validate the domain part of an email address      |

### `DomainService` methods

| Method                 | Description                                                     |
| ---------------------- | --------------------------------------------------------------- |
| `new(db, verifier)`    | Create a new domain service                                     |
| `register(tenant_id, domain)` | Register a domain claim (returns existing pending if any) |
| `verify(id)`           | Verify a claim via DNS TXT record                               |
| `remove(id)`           | Delete a domain claim                                           |
| `enable_email(id)`     | Enable email routing flag (requires verified status)            |
| `disable_email(id)`    | Disable email routing flag                                      |
| `enable_routing(id)`   | Enable HTTP request routing flag (requires verified status)     |
| `disable_routing(id)`  | Disable HTTP request routing flag                               |
| `lookup_email_domain(email)` | Find the tenant for a verified, email-enabled domain      |
| `lookup_routing_domain(domain)` | Find the tenant for a verified, routing-enabled domain |
| `resolve_tenant(domain)` | Convenience wrapper returning only the tenant ID              |
| `list(tenant_id)`      | List all domain claims for a tenant                             |

### Domain usage

```rust
use modo::tenant::domain::DomainService;
use modo::db::Database;
use modo::dns::DomainVerifier;

// Create the service
let svc = DomainService::new(db, verifier);

// Register a domain for a tenant
let claim = svc.register("tenant-1", "example.com").await?;
// claim.verification_token contains the DNS TXT value

// After the user sets up _modo-verify.example.com TXT record:
let claim = svc.verify(&claim.id).await?;
assert_eq!(claim.status, modo::tenant::domain::ClaimStatus::Verified);

// Enable capabilities
svc.enable_routing(&claim.id).await?;
svc.enable_email(&claim.id).await?;

// Look up tenant by domain
let tenant = svc.resolve_tenant("example.com").await?;
```

### Required database table

Applications must create the `tenant_domains` table in their own migrations.
The expected schema:

```sql
CREATE TABLE tenant_domains (
    id                 TEXT PRIMARY KEY,
    tenant_id          TEXT NOT NULL,
    domain             TEXT NOT NULL,
    verification_token TEXT NOT NULL,
    status             TEXT NOT NULL DEFAULT 'pending',
    use_for_email      INTEGER NOT NULL DEFAULT 0,
    use_for_routing    INTEGER NOT NULL DEFAULT 0,
    created_at         TEXT NOT NULL,
    verified_at        TEXT
);
```
