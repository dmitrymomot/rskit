# API Key Module Design — 2026-03-30

Prefixed API key issuance, verification, scoping, and lifecycle management. SQLite-backed, tenant-scoped.

## Key Format

```
modo_01JQXK5M3N8R4T6V2W9Y0Z8kJmNpQrStUvWxYz2A4B6C8D
^^^^                          ^^^^^^^^^^^^^^^^^^^^^^^^^^
prefix  ULID (26, Crockford)  secret (32, base62)
```

- **Prefix** — configurable per app (`modo`, `sk`, `pk`, etc.). Must be `[a-zA-Z0-9]`, 1–20 chars. Enables automated secret scanning (GitHub, GitGuardian detect leaked keys by prefix).
- **ULID** (26 chars) — Crockford base32, uppercase. Time-sortable, globally unique. Serves as the database primary key. Split from the secret by offset (first 26 chars of the body after `_`).
- **Secret** (configurable length, default 32, minimum 16, base62 chars) — `[0-9A-Za-z]`, ~190 bits entropy at default length. Shown once at creation. Stored as `hex(sha256(secret))`. Verified with constant-time comparison.

**Parsing:** split on first `_` → prefix + body. Body first 26 chars = ULID, remainder = secret.

**Display:** keys are identified by `id` (ULID) and `name` only. No part of the token (including the prefix+ULID fragment) is ever displayed back to the user after creation. The `name` field is the human-readable identifier.

## Feature Flag

```toml
apikey = ["db"]
```

No new dependencies — uses existing `encoding::hex`, `id::ulid`, `rand`, `subtle`.

## Config

```rust
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ApiKeyConfig {
    pub prefix: String,              // must be [a-zA-Z0-9], 1-20 chars
    pub secret_length: usize,        // minimum 16, default 32
    pub touch_threshold: Duration,   // default 1m
}
```

**Validation at construction:** `ApiKeyStore::new(db, config)` returns `Result<Self>`. Rejects:
- Prefix empty, >20 chars, or contains non-alphanumeric characters
- Secret length < 16

**YAML:**

```yaml
apikey:
  prefix: "modo"
  secret_length: 32
  touch_threshold: "1m"
```

## Types

```rust
/// What the caller provides to create a key.
pub struct CreateKeyRequest {
    pub tenant_id: String,           // required — every key belongs to a tenant
    pub name: String,                // "Production webhook key"
    pub scopes: Vec<String>,         // ["read:orders", "write:users"]
    pub expires_at: Option<String>,  // None = lifetime token, tenant controls this
}

/// Returned once at creation — contains the raw token.
pub struct ApiKeyCreated {
    pub id: String,                  // ULID
    pub raw_token: String,           // full key, show once, never retrievable
    pub name: String,
    pub scopes: Vec<String>,
    pub tenant_id: String,
    pub expires_at: Option<String>,
    pub created_at: String,
}

/// Stored form — used by backend trait. Crate-internal.
pub(crate) struct ApiKeyRecord {
    pub id: String,                  // ULID
    pub key_hash: String,            // hex(sha256(secret))
    pub tenant_id: String,
    pub name: String,
    pub scopes: Vec<String>,         // serialized as JSON in DB
    pub expires_at: Option<String>,
    pub last_used_at: Option<String>,
    pub created_at: String,
    pub revoked_at: Option<String>,
}

/// Public metadata — extracted by middleware, used in handlers.
/// No hash, no revoked_at.
#[derive(Debug, Clone, Serialize)]
pub struct ApiKeyMeta {
    pub id: String,                  // ULID
    pub tenant_id: String,
    pub name: String,
    pub scopes: Vec<String>,
    pub expires_at: Option<String>,
    pub last_used_at: Option<String>,
    pub created_at: String,
}
```

## Backend Trait (Thin Storage Primitives)

```rust
pub trait ApiKeyBackend: Send + Sync {
    /// Store a new key record.
    fn store(&self, record: &ApiKeyRecord) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>>;

    /// Look up a key by ULID. Returns None if not found.
    fn lookup(&self, key_id: &str) -> Pin<Box<dyn Future<Output = Result<Option<ApiKeyRecord>>> + Send + '_>>;

    /// Set revoked_at on a key.
    fn revoke(&self, key_id: &str, revoked_at: &str) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>>;

    /// List keys for a tenant.
    fn list(&self, tenant_id: &str) -> Pin<Box<dyn Future<Output = Result<Vec<ApiKeyRecord>>> + Send + '_>>;

    /// Update last_used_at timestamp.
    fn update_last_used(&self, key_id: &str, timestamp: &str) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>>;

    /// Update expires_at timestamp (refresh).
    fn update_expires_at(&self, key_id: &str, expires_at: Option<&str>) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>>;
}
```

The backend only does CRUD. All business logic lives in the wrapper:
- Key generation (ULID + secret + base62 encoding)
- SHA-256 hashing (no salt — ~190 bits entropy makes salt unnecessary)
- Constant-time comparison via `subtle` crate
- Expiry/revocation checks during verification
- Touch throttling
- Config validation

## Wrapper (`ApiKeyStore`) Public API

```rust
#[derive(Clone)]
pub struct ApiKeyStore(Arc<Inner>);

impl ApiKeyStore {
    /// Factory — validates config, creates built-in SQLite backend.
    pub fn new(db: Database, config: ApiKeyConfig) -> Result<Self>;

    /// Custom backend — validates config, uses provided storage.
    pub fn from_backend(backend: Arc<dyn ApiKeyBackend>, config: ApiKeyConfig) -> Result<Self>;

    /// Create a key. Returns raw token (shown once).
    pub async fn create(&self, req: &CreateKeyRequest) -> Result<ApiKeyCreated>;

    /// Verify a raw token. Returns metadata if valid.
    pub async fn verify(&self, raw_token: &str) -> Result<ApiKeyMeta>;

    /// Revoke a key by ID.
    pub async fn revoke(&self, key_id: &str) -> Result<()>;

    /// List all keys for a tenant (no secrets).
    pub async fn list(&self, tenant_id: &str) -> Result<Vec<ApiKeyMeta>>;

    /// Update expires_at (refresh/extend a key).
    pub async fn refresh(&self, key_id: &str, expires_at: Option<&str>) -> Result<()>;
}
```

### Verification Flow

1. Parse token → extract prefix, ULID, secret
2. Validate prefix matches config
3. `backend.lookup(ulid)` — single indexed primary key read
4. Check `revoked_at IS NULL`
5. Check `expires_at` is `None` or in the future
6. `hex(sha256(secret))` → constant-time compare against stored `key_hash`
7. On success, check touch threshold: if `last_used_at` is `None` or older than `now - touch_threshold` → fire-and-forget `tokio::spawn` call to `update_last_used`. Otherwise skip.
8. Convert `ApiKeyRecord` → `ApiKeyMeta`, return

### Touch Throttling

`touch()` is fire-and-forget via `tokio::spawn`. The `touch_threshold` config (default 1m) prevents a DB write on every authenticated request. At most one write per key per threshold interval. Failed touches are traced but never fail the request.

## Middleware & Extractors

### ApiKeyLayer

```rust
pub struct ApiKeyLayer { .. }

impl ApiKeyLayer {
    /// Extract from Authorization: Bearer <token>
    pub fn new(store: ApiKeyStore) -> Self;

    /// Extract from a custom header
    pub fn from_header(store: ApiKeyStore, header: &str) -> Self;
}
```

Header only — no query parameter support. Query params leak tokens into server logs and browser history. Apps that need query-param auth can extract manually and call `store.verify()`.

**Middleware flow:**
1. Read header (`Authorization: Bearer ...` or custom)
2. Missing header → `Error::unauthorized("missing API key")`
3. Call `store.verify(raw_token)`
4. On success → insert `ApiKeyMeta` into request extensions
5. On failure → return the error from `verify()`

All errors are `modo::Error`. The app's error handler decides rendering.

### Extractors

```rust
// Required — returns Error::unauthorized if no key in extensions
impl FromRequestParts for ApiKeyMeta { .. }

// Optional — returns None if no key in extensions
impl OptionalFromRequestParts for ApiKeyMeta { .. }
```

### Scope Guard

```rust
/// Route layer — checks if verified key has the required scope.
/// Exact string match against key.scopes.
/// Returns Error::forbidden("missing required scope: {scope}") if not.
pub fn require_scope(scope: &str) -> impl Layer;
```

Must be applied after `ApiKeyLayer`. Panics if `ApiKeyMeta` not in extensions (same pattern as `Session` extractor).

Exact string match only — no wildcards, no hierarchy. App handles complex scope logic in handler code. Matches modo's RBAC philosophy: framework stores and extracts, app defines meaning.

## Recommended Schema

Documented in module docs. Migration owned by end app.

```sql
CREATE TABLE api_keys (
    id            TEXT PRIMARY KEY,              -- ULID
    key_hash      TEXT NOT NULL,                 -- hex(sha256(secret))
    tenant_id     TEXT NOT NULL,
    name          TEXT NOT NULL,
    scopes        TEXT NOT NULL DEFAULT '[]',    -- JSON array
    expires_at    TEXT,                          -- NULL = lifetime token
    last_used_at  TEXT,
    created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    revoked_at    TEXT
);

CREATE INDEX idx_api_keys_tenant ON api_keys(tenant_id);
CREATE INDEX idx_api_keys_created ON api_keys(created_at);
```

## Error Handling

All errors are `modo::Error`. The app's error handler decides rendering. No raw HTTP responses constructed in middleware.

| Operation | Error | Constructor |
|-----------|-------|-------------|
| `new()` / `from_backend()` | Invalid config (bad prefix, short secret) | `Error::bad_request()` |
| `create()` | Missing/empty required fields | `Error::bad_request()` |
| `verify()` — malformed token | Can't parse prefix/ULID/secret | `Error::unauthorized("invalid API key")` |
| `verify()` — prefix mismatch | Token prefix doesn't match config | `Error::unauthorized("invalid API key")` |
| `verify()` — not found | ULID not in DB | `Error::unauthorized("invalid API key")` |
| `verify()` — revoked | `revoked_at` is set | `Error::unauthorized("invalid API key")` |
| `verify()` — expired | `expires_at` in the past | `Error::unauthorized("invalid API key")` |
| `verify()` — hash mismatch | Constant-time compare fails | `Error::unauthorized("invalid API key")` |
| `revoke()` — not found | Key ID doesn't exist | `Error::not_found()` |
| `refresh()` — not found | Key ID doesn't exist | `Error::not_found()` |
| `require_scope()` | Scope not in key's scopes | `Error::forbidden("missing required scope: {scope}")` |
| Middleware — missing header | No Authorization/custom header | `Error::unauthorized("missing API key")` |

All `verify()` failures return the same generic message — no distinction between not-found, revoked, expired, or wrong secret. Prevents enumeration.

## Testing Strategy

### Unit tests (no DB)

- Key generation — format validation, prefix + ULID(26) + secret(N) structure
- Token parsing — split prefix, extract ULID, extract secret, reject malformed tokens
- SHA-256 hashing — hash produces expected hex output
- Constant-time comparison — correct secret passes, wrong secret fails
- Config validation — reject bad prefixes, short secrets, accept valid configs
- Touch threshold logic — skip when recent, fire when stale

### Integration tests (with `TestDb`)

- Full create → verify → revoke lifecycle
- Create → verify → refresh → verify with new expiry
- Verify expired key returns unauthorized
- Verify revoked key returns unauthorized
- List keys by tenant returns correct set
- Touch updates last_used_at in DB
- Touch skipped when within threshold

### Middleware tests (with `TestApp`)

- Request with valid Bearer token → handler receives `ApiKeyMeta`
- Request without header → 401
- Request with invalid token → 401
- `Option<ApiKeyMeta>` → `None` when no middleware, `Some` when present
- `require_scope()` → 403 when scope missing, passes when present
- Custom header extraction works

### Test helpers

- `pub mod test` behind `#[cfg(any(test, feature = "apikey-test"))]`
- In-memory backend for unit tests
- Helper to create a key and return both the raw token and stored record

## File Layout

```
src/apikey/
    mod.rs          — pub mod imports + re-exports
    config.rs       — ApiKeyConfig, validation, Deserialize
    backend.rs      — ApiKeyBackend trait
    store.rs        — ApiKeyStore wrapper (generation, hashing, verification, touch logic)
    sqlite.rs       — built-in SQLite backend (implements ApiKeyBackend)
    types.rs        — CreateKeyRequest, ApiKeyCreated, ApiKeyMeta, ApiKeyRecord
    middleware.rs    — ApiKeyLayer, Tower Layer + Service impl
    extractor.rs    — FromRequestParts, OptionalFromRequestParts for ApiKeyMeta
    scope.rs        — require_scope() guard layer
    token.rs        — key generation, parsing, hashing (private functions)
```

## Usage Example

```rust
// --- Wiring ---
let config: modo::Config = modo::config::load("config")?;
let db = modo::db::connect(&config.database).await?;
let keys = ApiKeyStore::new(db.clone(), config.apikey)?;

let app = Router::new()
    // Key management (behind session auth, not API key auth)
    .route("/api/keys", post(create_key))
    .route("/api/keys", get(list_keys))
    .route("/api/keys/{id}/revoke", post(revoke_key))
    .route("/api/keys/{id}/refresh", post(refresh_key))
    // API key protected endpoints
    .route("/api/orders", get(list_orders))
    .route_layer(require_scope("read:orders"))
    .layer(ApiKeyLayer::new(keys.clone()))
    .with_service(keys);

// --- Create a key ---
async fn create_key(
    session: Session,
    Service(keys): Service<ApiKeyStore>,
    body: JsonRequest<CreateKeyForm>,
) -> Result<Json<ApiKeyCreated>> {
    let created = keys.create(&CreateKeyRequest {
        tenant_id: session.tenant_id().into(),
        name: body.name.clone(),
        scopes: body.scopes.clone(),
        expires_at: body.expires_at.clone(),
    }).await?;
    Ok(Json(created))
}

// --- List keys for tenant ---
async fn list_keys(
    session: Session,
    Service(keys): Service<ApiKeyStore>,
) -> Result<Json<Vec<ApiKeyMeta>>> {
    let list = keys.list(session.tenant_id()).await?;
    Ok(Json(list))
}

// --- Protected endpoint ---
async fn list_orders(key: ApiKeyMeta) -> Result<Json<Vec<Order>>> {
    // key.tenant_id, key.scopes available
}
```

## Design Notes

- `ApiKeyStore` wraps `Arc<Inner>` — cheap to clone, testable with in-memory backend
- Backend trait is thin (storage primitives only) — security-critical logic (hashing, constant-time comparison, token generation) lives in the wrapper, never duplicated across backends
- Tenant-scoped — every key belongs to a tenant, `tenant_id` is required and NOT NULL
- No rotation — apps create new keys and revoke old ones. `refresh()` updates `expires_at` for extending key lifetime.
- No owner tracking — the module manages keys per tenant. Apps track key→creator in their own domain tables if needed.
- Scopes are `Vec<String>` with exact match — framework stores and extracts, app defines meaning
- SHA-256 without salt — ~190 bits entropy makes salt unnecessary, same rationale as choosing SHA-256 over argon2
- Touch throttling via `touch_threshold` — at most one DB write per key per interval, fire-and-forget via `tokio::spawn`
- Header-only extraction — no query param support to prevent token leakage
- All errors are `modo::Error` — app's error handler decides rendering
- No new dependencies — uses existing `encoding::hex`, `id::ulid`, `rand`, `subtle`
