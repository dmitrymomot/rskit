# apikey

Prefixed API key issuance, verification, scoping, and lifecycle management.

Requires the `apikey` feature flag (depends on `db`):

```toml
[dependencies]
modo = { version = "0.6", features = ["apikey"] }
```

## Key types

### Core

| Type | Purpose |
|------|---------|
| `ApiKeyStore` | Tenant-scoped store: create, verify, revoke, list, refresh keys |
| `ApiKeyConfig` | YAML-deserializable configuration (prefix, secret length, touch threshold) |
| `ApiKeyBackend` | Trait for pluggable storage backends (SQLite built-in) |

### Middleware

| Type | Purpose |
|------|---------|
| `ApiKeyLayer` | Tower layer that verifies API keys on incoming requests |
| `require_scope` | Tower layer factory that enforces a required scope on verified keys |

### Data types

| Type | Purpose |
|------|---------|
| `ApiKeyMeta` | Public metadata extracted by middleware, usable as an axum extractor |
| `ApiKeyCreated` | One-time creation result containing the raw token |
| `ApiKeyRecord` | Full stored record used by backend implementations |
| `CreateKeyRequest` | Input for `ApiKeyStore::create` |

### Testing

| Type | Purpose |
|------|---------|
| `test::InMemoryBackend` | In-memory backend for unit tests (requires `test-helpers` feature) |

## Usage

### Creating a store

```rust,ignore
use modo::apikey::{ApiKeyConfig, ApiKeyStore};

let store = ApiKeyStore::new(db, ApiKeyConfig::default())?;
```

Use `ApiKeyStore::from_backend` to supply a custom `ApiKeyBackend` implementation
instead of the built-in SQLite backend:

```rust,ignore
use std::sync::Arc;
use modo::apikey::{ApiKeyConfig, ApiKeyStore, ApiKeyBackend};

let backend: Arc<dyn ApiKeyBackend> = /* your backend */;
let store = ApiKeyStore::from_backend(backend, ApiKeyConfig::default())?;
```

### Issuing a key

```rust,ignore
use modo::apikey::CreateKeyRequest;

let created = store.create(&CreateKeyRequest {
    tenant_id: "tenant_abc".into(),
    name: "CI deploy key".into(),
    scopes: vec!["write:deploys".into()],
    expires_at: Some("2027-01-01T00:00:00Z".into()),
}).await?;

// Show `created.raw_token` to the user exactly once.
// The raw token is not retrievable after creation.
println!("Token: {}", created.raw_token);
```

### Verifying a key

```rust,ignore
let meta = store.verify("modo_01JQXK5M3N...secret").await?;
println!("tenant={} scopes={:?}", meta.tenant_id, meta.scopes);
```

Verification checks (in order): token format, existence, revocation status,
expiration, and constant-time hash comparison. All failures return the same
generic `unauthorized` error to prevent enumeration.

### Protecting routes with middleware

Apply `ApiKeyLayer` to verify the `Authorization: Bearer <token>` header:

```rust,ignore
use axum::{Router, routing::get};
use modo::apikey::{ApiKeyLayer, ApiKeyStore};

let app: Router = Router::new()
    .route("/api/resource", get(handler))
    .layer(ApiKeyLayer::new(store));
```

Read from a custom header instead:

```rust,ignore
let layer = ApiKeyLayer::from_header(store, "x-api-key")?;
```

### Requiring scopes

Apply `require_scope` as a route layer **after** `ApiKeyLayer`:

```rust,ignore
use axum::{Router, routing::get};
use modo::auth::guard::require_scope;

let app: Router = Router::new()
    .route("/orders", get(list_orders))
    .route_layer(require_scope("read:orders"));
```

### Extracting key metadata in handlers

`ApiKeyMeta` implements `FromRequestParts` and `OptionalFromRequestParts`:

```rust,ignore
use modo::apikey::ApiKeyMeta;

async fn handler(meta: ApiKeyMeta) -> String {
    format!("Hello tenant {}", meta.tenant_id)
}

// Or optionally:
async fn maybe_handler(meta: Option<ApiKeyMeta>) -> String {
    match meta {
        Some(m) => format!("Authenticated: {}", m.tenant_id),
        None => "Anonymous".into(),
    }
}
```

### Revoking and listing keys

```rust,ignore
// Revoke a key by ID
store.revoke("01JQXK5M3N8R4T6V2W9Y0ZABCD").await?;

// List all active keys for a tenant (returns Vec<ApiKeyMeta>)
let keys = store.list("tenant_abc").await?;

// Refresh expiration
store.refresh("01JQXK5M3N8R4T6V2W9Y0ZABCD", Some("2028-06-01T00:00:00Z")).await?;
```

## Configuration

```yaml
apikey:
  # Key prefix prepended before the underscore separator.
  # Must be ASCII alphanumeric, 1-20 characters. Default: "modo"
  prefix: "modo"

  # Length of the random secret portion in base62 characters.
  # Minimum 16. Default: 32
  secret_length: 32

  # Minimum interval between last_used_at updates, in seconds.
  # Default: 60
  touch_threshold_secs: 60
```

## Error handling

All errors are returned as `modo::Error` with appropriate HTTP status codes:

| Method | HTTP status | When |
|--------|-------------|------|
| `ApiKeyStore::create` | 400 Bad Request | `tenant_id` or `name` is empty, or `expires_at` is not valid RFC 3339 |
| `ApiKeyStore::verify` | 401 Unauthorized | Token is malformed, not found, revoked, expired, or hash mismatch |
| `ApiKeyStore::revoke` | 404 Not Found | No key with the given ID exists |
| `ApiKeyStore::refresh` | 400 Bad Request | `expires_at` is not valid RFC 3339 |
| `ApiKeyStore::refresh` | 404 Not Found | No key with the given ID exists |
| `ApiKeyLayer` | 401 Unauthorized | Missing or invalid `Authorization` header |
| `require_scope` | 403 Forbidden | Verified key lacks the required scope |
| `require_scope` | 500 Internal | `require_scope` applied without `ApiKeyLayer` |

Verification deliberately returns the same generic "invalid API key" message
for all failure cases to prevent enumeration attacks.

## Database schema

The module does not ship migrations. Applications must create the `api_keys`
table. Required columns:

```sql
CREATE TABLE api_keys (
    id           TEXT PRIMARY KEY,
    key_hash     TEXT NOT NULL,
    tenant_id    TEXT NOT NULL,
    name         TEXT NOT NULL,
    scopes       TEXT NOT NULL DEFAULT '[]',
    expires_at   TEXT,
    last_used_at TEXT,
    created_at   TEXT NOT NULL,
    revoked_at   TEXT
);
```
