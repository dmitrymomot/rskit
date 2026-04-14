# API Keys

Prefixed API key issuance, verification, scoping, and lifecycle management. Always available.

Import types from `modo::auth::apikey`:

```rust
use modo::auth::apikey::{
    ApiKeyBackend, ApiKeyConfig, ApiKeyCreated, ApiKeyLayer, ApiKeyMeta,
    ApiKeyRecord, ApiKeyStore, CreateKeyRequest,
};
use modo::auth::guard::require_scope;
```

`InMemoryBackend` is only available under `#[cfg(test)]` or `feature = "test-helpers"`:

```rust
use modo::auth::apikey::test::InMemoryBackend;
```

Source: `src/auth/apikey/` (mod.rs, config.rs, types.rs, token.rs, backend.rs, sqlite.rs, store.rs, middleware.rs, extractor.rs). Scope gating lives in `src/auth/guard.rs`.

---

## ApiKeyConfig

YAML-deserializable configuration. All fields have defaults — an empty `apikey:` block is valid.

```rust
#[non_exhaustive]
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ApiKeyConfig {
    pub prefix: String,              // default: "modo"
    pub secret_length: usize,        // default: 32
    pub touch_threshold_secs: u64,   // default: 60
}
```

- `prefix` — prepended before the underscore separator. Must be `[a-zA-Z0-9]`, 1–20 chars.
- `secret_length` — length of the random base62 secret. Minimum 16.
- `touch_threshold_secs` — minimum interval between `last_used_at` updates.

### validate(&self) -> Result<()>

Validates prefix format and secret length. Called automatically by `ApiKeyStore::new()`.

Returns `bad_request` if prefix is empty, too long, contains non-alphanumeric chars, or if secret_length < 16.

### YAML example

```yaml
apikey:
  prefix: "modo"
  secret_length: 32
  touch_threshold_secs: 60
```

---

## CreateKeyRequest

Input for `ApiKeyStore::create`. Not `Deserialize` — handlers construct it manually (tenant_id typically comes from auth context, not request body).

```rust
pub struct CreateKeyRequest {
    pub tenant_id: String,
    pub name: String,
    pub scopes: Vec<String>,
    pub expires_at: Option<String>,  // RFC 3339, or None for lifetime
}
```

---

## ApiKeyCreated

Returned once by `ApiKeyStore::create`. Contains the raw token — show it to the user, never retrievable again.

```rust
#[derive(Debug, Clone, Serialize)]
pub struct ApiKeyCreated {
    pub id: String,
    pub raw_token: String,
    pub name: String,
    pub scopes: Vec<String>,
    pub tenant_id: String,
    pub expires_at: Option<String>,
    pub created_at: String,
}
```

---

## ApiKeyMeta

Public metadata extracted by middleware. Implements `FromRequestParts` (returns `unauthorized` if missing) and `OptionalFromRequestParts` (returns `None` if missing). Also implements `Serialize`.

```rust
#[derive(Debug, Clone, Serialize)]
pub struct ApiKeyMeta {
    pub id: String,
    pub tenant_id: String,
    pub name: String,
    pub scopes: Vec<String>,
    pub expires_at: Option<String>,
    pub last_used_at: Option<String>,
    pub created_at: String,
}
```

Use as an axum extractor in handlers:

```rust
// Required — returns 401 if no API key
async fn handler(meta: ApiKeyMeta) { /* ... */ }

// Optional — None if no API key middleware applied
async fn handler(meta: Option<ApiKeyMeta>) { /* ... */ }
```

---

## ApiKeyRecord

Full stored record used by backend implementations. Contains hash and revocation fields not exposed in `ApiKeyMeta`.

```rust
#[derive(Clone)]
pub struct ApiKeyRecord {
    pub id: String,
    pub key_hash: String,
    pub tenant_id: String,
    pub name: String,
    pub scopes: Vec<String>,
    pub expires_at: Option<String>,
    pub last_used_at: Option<String>,
    pub created_at: String,
    pub revoked_at: Option<String>,
}
```

### into_meta(self) -> ApiKeyMeta

Converts to public metadata, dropping `key_hash` and `revoked_at`.

---

## ApiKeyBackend

Thin storage trait. Implementations handle only CRUD — all business logic lives in `ApiKeyStore`. Uses `Pin<Box<dyn Future>>` for object safety behind `Arc<dyn ApiKeyBackend>`.

```rust
pub trait ApiKeyBackend: Send + Sync {
    fn store(&self, record: &ApiKeyRecord)
        -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>>;
    fn lookup(&self, key_id: &str)
        -> Pin<Box<dyn Future<Output = Result<Option<ApiKeyRecord>>> + Send + '_>>;
    fn revoke(&self, key_id: &str, revoked_at: &str)
        -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>>;
    fn list(&self, tenant_id: &str)
        -> Pin<Box<dyn Future<Output = Result<Vec<ApiKeyRecord>>> + Send + '_>>;
    fn update_last_used(&self, key_id: &str, timestamp: &str)
        -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>>;
    fn update_expires_at(&self, key_id: &str, expires_at: Option<&str>)
        -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>>;
}
```

Built-in: SQLite backend (used automatically by `ApiKeyStore::new`).

For custom backends, use `ApiKeyStore::from_backend(Arc::new(my_backend), config)`.

---

## ApiKeyStore

Tenant-scoped store wrapping a backend. Handles key generation, SHA-256 hashing, constant-time verification, touch throttling. Cheap to clone (`Arc<Inner>`).

```rust
pub struct ApiKeyStore(Arc<Inner>);
```

### new(db: Database, config: ApiKeyConfig) -> Result\<Self\>

Create from the built-in SQLite backend. Validates config at construction.

Returns `bad_request` if config validation fails.

### from_backend(backend: Arc\<dyn ApiKeyBackend\>, config: ApiKeyConfig) -> Result\<Self\>

Create from a custom backend. Validates config at construction.

### async create(&self, req: &CreateKeyRequest) -> Result\<ApiKeyCreated\>

Generate a new API key. Returns the raw token (shown once, never stored).

Token format: `{prefix}_{ulid}{base62_secret}` (e.g., `modo_01JQXK5M3N...abcdef`).

Returns `bad_request` if `tenant_id` or `name` is empty, or if `expires_at` is not valid RFC 3339.

### async verify(&self, raw_token: &str) -> Result\<ApiKeyMeta\>

Verify a raw token. All failure cases return the same `unauthorized` error to prevent enumeration.

Checks: parse token → lookup by ULID → check revoked → check expired → constant-time hash comparison → fire-and-forget touch update.

### async revoke(&self, key_id: &str) -> Result<()>

Revoke a key by ULID. Returns `not_found` if the key doesn't exist.

### async list(&self, tenant_id: &str) -> Result\<Vec\<ApiKeyMeta\>\>

List all active (non-revoked) keys for a tenant. Ordered by `created_at DESC`.

### async refresh(&self, key_id: &str, expires_at: Option<&str>) -> Result<()>

Update a key's expiration. Pass `None` to make it a lifetime key. Validates RFC 3339 format.

Returns `not_found` if the key doesn't exist, `bad_request` if expires_at is malformed.

---

## ApiKeyLayer

Tower `Layer` that verifies API keys on incoming requests. Reads the token, calls `ApiKeyStore::verify`, and inserts `ApiKeyMeta` into request extensions.

```rust
pub struct ApiKeyLayer { /* private */ }
```

### new(store: ApiKeyStore) -> Self

Read from `Authorization: Bearer <token>` header.

### from_header(store: ApiKeyStore, header: &str) -> Result\<Self\>

Read from a custom header (e.g., `x-api-key`). The header value is used directly (no `Bearer` prefix).

Returns `bad_request` if the header name is invalid.

### Usage

```rust
use modo::auth::apikey::{ApiKeyStore, ApiKeyConfig, ApiKeyLayer};
use axum::Router;

let store = ApiKeyStore::new(db, ApiKeyConfig::default()).unwrap();

// Bearer header (default)
let app = Router::new()
    .route("/api/v1/orders", get(list_orders))
    .layer(ApiKeyLayer::new(store.clone()));

// Custom header
let app = Router::new()
    .route("/api/v1/orders", get(list_orders))
    .layer(ApiKeyLayer::from_header(store, "x-api-key").unwrap());
```

---

## require_scope

Tower layer factory that enforces a required scope on the verified API key. Must be applied after `ApiKeyLayer` (as a route layer).

```rust
pub fn require_scope(scope: &str) -> ScopeLayer
```

Uses exact string matching. Returns `403 Forbidden` if the key lacks the scope. Returns `500 Internal Server Error` if `ApiKeyMeta` is not in request extensions (i.e., `ApiKeyLayer` was not applied).

### Usage

```rust
use modo::auth::apikey::ApiKeyLayer;
use modo::auth::guard::require_scope;
use axum::{Router, routing::get};

let app: Router = Router::new()
    .route("/orders", get(list_orders))
    .route_layer(require_scope("read:orders"))
    .route("/admin", get(admin_panel))
    .route_layer(require_scope("admin"))
    .layer(ApiKeyLayer::new(store));
```

---

## InMemoryBackend (test helper)

In-memory `ApiKeyBackend` for unit tests. Available under `#[cfg(test)]` or `feature = "test-helpers"`.

```rust
use modo::auth::apikey::test::InMemoryBackend;
use modo::auth::apikey::{ApiKeyStore, ApiKeyConfig};
use std::sync::Arc;

let backend = Arc::new(InMemoryBackend::new());
let store = ApiKeyStore::from_backend(backend, ApiKeyConfig::default()).unwrap();
```

---

## SQL Schema

The module does not ship migrations — end apps own the schema. Required table:

```sql
CREATE TABLE api_keys (
    id            TEXT PRIMARY KEY,
    key_hash      TEXT NOT NULL,
    tenant_id     TEXT NOT NULL,
    name          TEXT NOT NULL,
    scopes        TEXT NOT NULL DEFAULT '[]',
    expires_at    TEXT,
    last_used_at  TEXT,
    created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    revoked_at    TEXT
);
CREATE INDEX idx_api_keys_tenant ON api_keys(tenant_id);
CREATE INDEX idx_api_keys_created ON api_keys(created_at);
```

`scopes` is stored as a JSON array of strings.

---

## Gotchas

- **Token shown once** — `ApiKeyCreated.raw_token` is the only time the plaintext token is available. The database stores only the SHA-256 hash.
- **Scope matching is case-sensitive** — `"read:orders"` and `"Read:Orders"` are different scopes.
- **Touch is best-effort** — `last_used_at` updates are fire-and-forget (`tokio::spawn`). May be lost on shutdown.
- **Expiry uses RFC 3339** — both `create()` and `refresh()` validate the format. Lexicographic comparison is not used — timestamps are parsed with `chrono`.
- **`require_scope` needs `ApiKeyLayer`** — if applied without the API key middleware, returns 500 (not a panic).
- **`list()` excludes revoked keys** — only active keys are returned.
- **No rate limiting on verify** — the module doesn't limit failed attempts. Apps must add rate limiting via middleware.
