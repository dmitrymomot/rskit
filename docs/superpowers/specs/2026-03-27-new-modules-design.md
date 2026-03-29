# New Modules Design — 2026-03-27 (Revised 2026-03-30)

High-level design for 5 new modo framework modules: audit logging, API keys, tier-based gating, embeddings, and data export.

## Changes from Original Spec

- **Removed:** Pagination, filtering, HTTP client — already implemented in `src/db/page.rs`, `src/db/filter.rs`, `src/http/`
- **Removed:** Domain-verified signup — already implemented in `src/tenant/domain.rs`
- **Removed:** Feature flags — dropped in favor of tier-based gating for product features
- **Added:** Embeddings (`embed`) — text-to-vector via LLM provider APIs
- **Added:** API keys (`apikey`) — prefixed key issuance, verification, scoping, rotation

## Build Phases

```
Phase 1 (independent, parallel):
  audit    — standalone, needs db
  apikey   — standalone, needs db

Phase 2 (independent, parallel):
  embed    — needs http-client (already exists)
  tier     — standalone, works with tenant module

Phase 3:
  export   — composes with db (filter + pagination already there)
```

## Dependency Map

```
audit  ← standalone, uses db
apikey ← standalone, uses db + encoding + rand

embed  ← uses http-client (existing)
tier   ← standalone, works with tenant module

export ← uses db (filter + pagination already exist), csv crate
```

## Design Patterns (All Modules)

All modules follow the **concrete-wrapper pattern**:

```rust
// Public backend trait for custom implementations
pub trait FooBackend: Send + Sync { .. }

// Concrete wrapper — Arc internally, cheap to clone
#[derive(Clone)]
pub struct Foo(Arc<dyn FooBackend>);

impl Foo {
    pub fn new(db: Database) -> Self;                           // built-in SQLite backend
    pub fn from_backend(backend: Arc<dyn FooBackend>) -> Self;  // custom backend
}
```

Handler ergonomics: `Service(foo): Service<Foo>` — no `Arc<dyn>` in signatures.

Middleware/guards return `modo::Error` — the app's error handler decides rendering.

---

## 1. Audit Logging (`audit`)

**Purpose:** Explicit event logging for business-significant actions. SQLite-backed with a universal schema that works for both single-tenant and multi-tenant apps.

**Feature flag:** None — always available (requires `db`).

### Public API

```rust
// Backend trait
pub trait AuditLogBackend: Send + Sync {
    fn record(&self, entry: &AuditEntry) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>>;
}

// Concrete wrapper
#[derive(Clone)]
pub struct AuditLog(Arc<dyn AuditLogBackend>);

impl AuditLog {
    /// Create from the built-in SQLite backend.
    pub fn new(db: Database) -> Self;

    /// Create from a custom backend.
    pub fn from_backend(backend: Arc<dyn AuditLogBackend>) -> Self;

    /// Record an audit event.
    pub async fn record(&self, entry: &AuditEntry) -> Result<()>;
}

// Entry (what the caller provides)
pub struct AuditEntry {
    pub actor: String,              // who: user ID, "system", API key ID
    pub action: String,             // what: "user.role.changed", "account.deleted"
    pub resource_type: String,      // on what kind: "user", "tenant", "api_key"
    pub resource_id: String,        // on which one: "usr_01ABC..."
    pub metadata: serde_json::Value,// extra context: {"old_role": "editor", "new_role": "admin"}
    pub ip: Option<String>,         // client IP if available
    pub tenant_id: Option<String>,  // NULL for single-tenant apps, set for multi-tenant
}

// Record (stored form — entry fields + id + timestamp)
pub struct AuditRecord {
    pub id: String,                 // ULID
    pub actor: String,
    pub action: String,
    pub resource_type: String,
    pub resource_id: String,
    pub metadata: serde_json::Value,
    pub ip: Option<String>,
    pub tenant_id: Option<String>,
    pub created_at: String,
}
```

### Repository (Query Interface)

```rust
#[derive(Clone)]
pub struct AuditRepo { .. } // Arc<Inner>

impl AuditRepo {
    pub fn new(db: Database) -> Self;

    pub async fn list(&self, req: &PageRequest) -> Result<Page<AuditRecord>>;
    pub async fn by_actor(&self, actor: &str, req: &PageRequest) -> Result<Page<AuditRecord>>;
    pub async fn by_resource(&self, resource_type: &str, resource_id: &str, req: &PageRequest) -> Result<Page<AuditRecord>>;
    pub async fn by_tenant(&self, tenant_id: &str, req: &PageRequest) -> Result<Page<AuditRecord>>;
    pub async fn by_action(&self, action: &str, req: &PageRequest) -> Result<Page<AuditRecord>>;

    /// Composes with filter module.
    pub async fn query(&self, filter: &ValidatedFilter, req: &PageRequest) -> Result<Page<AuditRecord>>;
}
```

### Recommended Schema

Documented in module docs. Migration owned by end app.

```sql
CREATE TABLE audit_log (
    id          TEXT PRIMARY KEY,           -- ULID
    actor       TEXT NOT NULL,
    action      TEXT NOT NULL,
    resource_type TEXT NOT NULL,
    resource_id TEXT NOT NULL,
    metadata    TEXT NOT NULL DEFAULT '{}', -- JSON
    ip          TEXT,
    tenant_id   TEXT,                       -- NULL for single-tenant
    created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE INDEX idx_audit_log_actor ON audit_log(actor);
CREATE INDEX idx_audit_log_resource ON audit_log(resource_type, resource_id);
CREATE INDEX idx_audit_log_action ON audit_log(action);
CREATE INDEX idx_audit_log_tenant ON audit_log(tenant_id) WHERE tenant_id IS NOT NULL;
CREATE INDEX idx_audit_log_created ON audit_log(created_at);
```

### Usage

```rust
// --- Recording in a handler ---

async fn change_role(
    session: Session,
    Service(audit): Service<AuditLog>,
    Service(users): Service<UserService>,
    Path(user_id): Path<String>,
    body: JsonRequest<ChangeRoleForm>,
) -> Result<()> {
    let old_role = users.get_role(&user_id).await?;
    users.set_role(&user_id, &body.role).await?;

    audit.record(&AuditEntry {
        actor: session.user_id().into(),
        action: "user.role.changed".into(),
        resource_type: "user".into(),
        resource_id: user_id,
        metadata: serde_json::json!({
            "old_role": old_role,
            "new_role": body.role,
        }),
        ip: None,
        tenant_id: None,
    }).await?;

    Ok(())
}

// --- Querying audit history ---

async fn user_audit_trail(
    Path(user_id): Path<String>,
    page: PageRequest,
    Service(repo): Service<AuditRepo>,
) -> Result<Json<Page<AuditRecord>>> {
    let results = repo.by_actor(&user_id, &page).await?;
    Ok(Json(results))
}

// --- Querying with filters ---
// GET /api/audit?action=user.role.changed&sort=-created_at&page=1&per_page=50

async fn search_audit(
    filter: Filter,
    page: PageRequest,
    Service(repo): Service<AuditRepo>,
) -> Result<Json<Page<AuditRecord>>> {
    let schema = FilterSchema::new()
        .field("actor", FieldType::Text)
        .field("action", FieldType::Text)
        .field("resource_type", FieldType::Text)
        .field("resource_id", FieldType::Text)
        .field("tenant_id", FieldType::Text)
        .field("created_at", FieldType::Date)
        .sortable(&["created_at", "action"]);

    let validated = filter.validate(&schema)?;
    let results = repo.query(&validated, &page).await?;
    Ok(Json(results))
}

// --- Wiring ---

let audit_log = AuditLog::new(db.clone());
let audit_repo = AuditRepo::new(db.clone());

let app = Router::new()
    .route("/api/users/{id}/role", put(change_role))
    .route("/api/users/{id}/audit", get(user_audit_trail))
    .route("/api/audit", get(search_audit))
    .with_service(audit_log)
    .with_service(audit_repo);
```

### Design Notes

- `AuditLog` wraps `Arc<dyn AuditLogBackend>` — testable with in-memory impls
- Handlers explicitly log what matters — no automatic middleware capture
- Actions use dot-notation by convention: `resource.verb` or `resource.sub.verb`
- `metadata` is unstructured JSON — keeps the schema stable while allowing arbitrary context
- `tenant_id` is NULL for single-tenant apps; the partial index on `tenant_id` has zero overhead when all values are NULL
- `record()` errors are traced but don't fail the request (configurable)

---

## 2. API Keys (`apikey`)

**Purpose:** Prefixed API key issuance, verification, scoping, and rotation. SQLite-backed.

**Feature flag:** None — always available (requires `db`).

### Key Format

```
modo_01JQXK5M3N8R4T6V2W9Y0Z8kJmNpQrStUvWxYz2A4B6C8D
^     ^                        ^
prefix ulid(26, offset-split)   secret(32, base62)
```

- **Prefix** — configurable per app (`modo`, `sk`, `pk`, etc.). Enables automated secret scanning (GitHub, GitGuardian detect leaked keys by prefix).
- **ULID** (26 chars) — time-sortable, globally unique. Serves as the database primary key. Used for lookup and display ("key `01JQXK5M...`"). Split from the secret by offset, not delimiter.
- **Secret** (32 chars, base62, ~190 bits entropy) — shown once at creation. Stored as SHA-256 hash + salt. Verified with constant-time comparison.

Single underscore after the prefix. Everything after it is the token body split programmatically by known ULID length (26 chars).

### Public API

```rust
// Backend trait
pub trait ApiKeyBackend: Send + Sync {
    fn create(&self, req: &CreateKeyRequest) -> Pin<Box<dyn Future<Output = Result<ApiKeyCreated>> + Send + '_>>;
    fn verify(&self, raw_token: &str) -> Pin<Box<dyn Future<Output = Result<ApiKeyMeta>> + Send + '_>>;
    fn revoke(&self, key_id: &str) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>>;
    fn list(&self, owner_id: &str) -> Pin<Box<dyn Future<Output = Result<Vec<ApiKeyMeta>>> + Send + '_>>;
    fn rotate(&self, key_id: &str) -> Pin<Box<dyn Future<Output = Result<ApiKeyCreated>> + Send + '_>>;
    fn touch(&self, key_id: &str) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>>;
}

// Concrete wrapper
#[derive(Clone)]
pub struct ApiKeyStore(Arc<dyn ApiKeyBackend>);

impl ApiKeyStore {
    pub fn new(db: Database, config: ApiKeyConfig) -> Self;
    pub fn from_backend(backend: Arc<dyn ApiKeyBackend>) -> Self;

    /// Create a key. Returns the full raw token (shown once).
    pub async fn create(&self, req: &CreateKeyRequest) -> Result<ApiKeyCreated>;

    /// Verify a raw token. Returns key metadata if valid.
    pub async fn verify(&self, raw_token: &str) -> Result<ApiKeyMeta>;

    /// Revoke a key by ID.
    pub async fn revoke(&self, key_id: &str) -> Result<()>;

    /// List keys for an owner (secrets never returned).
    pub async fn list(&self, owner_id: &str) -> Result<Vec<ApiKeyMeta>>;

    /// Rotate: create new key, set expiry on old key.
    pub async fn rotate(&self, key_id: &str) -> Result<ApiKeyCreated>;

    /// Update last_used_at (called by middleware automatically).
    pub async fn touch(&self, key_id: &str) -> Result<()>;
}
```

### Types

```rust
pub struct ApiKeyConfig {
    pub prefix: String,           // "modo"
    pub secret_length: usize,     // 32 (base62 chars)
}

pub struct CreateKeyRequest {
    pub owner_id: String,
    pub name: String,             // "Production webhook key"
    pub scopes: Vec<String>,      // ["read:orders", "write:users"]
    pub expires_at: Option<String>,
    pub tenant_id: Option<String>,
}

/// Returned once at creation — contains the raw token.
pub struct ApiKeyCreated {
    pub id: String,               // the ULID (also the short part of the token)
    pub raw_token: String,        // full key, show once
    pub key_prefix: String,       // "modo_01JQXK5M3N8R4T6V2W9Y0Z" for display
    pub name: String,
    pub scopes: Vec<String>,
    pub expires_at: Option<String>,
    pub created_at: String,
}

/// Stored metadata — no secrets.
#[derive(Debug, Clone, Serialize)]
pub struct ApiKeyMeta {
    pub id: String,               // ULID
    pub key_prefix: String,       // "modo_01JQXK5M3N8R4T6V2W9Y0Z"
    pub owner_id: String,
    pub name: String,
    pub scopes: Vec<String>,
    pub tenant_id: Option<String>,
    pub expires_at: Option<String>,
    pub last_used_at: Option<String>,
    pub created_at: String,
    pub revoked_at: Option<String>,
}
```

### Middleware & Extractors

```rust
// Layer — extracts key from request, verifies, injects ApiKeyMeta into extensions
pub struct ApiKeyLayer { .. }
impl ApiKeyLayer {
    pub fn new(store: ApiKeyStore) -> Self;                    // Authorization: Bearer
    pub fn from_header(store: ApiKeyStore, header: &str) -> Self; // custom header
}

// Extractor — pull verified key metadata in handlers
// impl FromRequestParts for ApiKeyMeta

// Scope guard
pub fn require_scope(scope: &str) -> impl Layer;
// returns Error::forbidden("Missing required scope: read:orders")
```

### Configuration (YAML)

```yaml
apikey:
    prefix: "modo"
    secret_length: 32
```

### Recommended Schema

```sql
CREATE TABLE api_keys (
    id          TEXT PRIMARY KEY,           -- ULID (the short part of the token)
    key_hash    TEXT NOT NULL,              -- hex(sha256(salt + secret))
    salt        TEXT NOT NULL,              -- random salt, hex-encoded
    owner_id    TEXT NOT NULL,
    name        TEXT NOT NULL,
    scopes      TEXT NOT NULL DEFAULT '[]', -- JSON array
    tenant_id   TEXT,
    expires_at  TEXT,
    last_used_at TEXT,
    created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    revoked_at  TEXT
);

CREATE INDEX idx_api_keys_owner ON api_keys(owner_id);
CREATE INDEX idx_api_keys_tenant ON api_keys(tenant_id) WHERE tenant_id IS NOT NULL;
```

### Verification Flow

1. Parse raw token: split on first `_` to get prefix, then split remainder by offset (first 26 chars = ULID, rest = secret)
2. Look up by ULID (indexed primary key)
3. Check `revoked_at IS NULL` and `expires_at > now` (or NULL)
4. Compute `sha256(salt + secret)`, constant-time compare against stored `key_hash`
5. On success, call `touch()` to update `last_used_at`
6. Inject `ApiKeyMeta` into request extensions

### Rotation Flow

1. Load existing key metadata by `key_id`
2. Create a new key with same `owner_id`, `scopes`, `tenant_id`
3. Set `expires_at` on the old key (configurable grace period, e.g., 7 days)
4. Return the new `ApiKeyCreated`
5. Both keys are valid during the grace period

### Usage

```rust
// --- Create a key ---
async fn create_key(
    session: Session,
    Service(keys): Service<ApiKeyStore>,
    body: JsonRequest<CreateKeyForm>,
) -> Result<Json<ApiKeyCreated>> {
    let created = keys.create(&CreateKeyRequest {
        owner_id: session.user_id().into(),
        name: body.name.clone(),
        scopes: body.scopes.clone(),
        expires_at: body.expires_at.clone(),
        tenant_id: None,
    }).await?;
    Ok(Json(created))
}

// --- Protected endpoint using API key auth ---
async fn list_orders(
    key: ApiKeyMeta,
) -> Result<Json<Vec<Order>>> {
    // key.owner_id, key.scopes available
}

// --- Wiring ---
let keys = ApiKeyStore::new(db.clone(), apikey_config);

let app = Router::new()
    .route("/api/keys", post(create_key))
    .route("/api/orders", get(list_orders))
    .route_layer(require_scope("read:orders"))
    .layer(ApiKeyLayer::new(keys.clone()))
    .with_service(keys);
```

### Design Notes

- SHA-256 + salt (not argon2) — API keys have high entropy (~190 bits), slow hashing is unnecessary and harmful to throughput
- Constant-time comparison via `subtle` crate (already in deps)
- Lookup by ULID primary key — single indexed lookup, no full-table scan
- Two active keys per owner for zero-downtime rotation
- `touch()` called automatically by `ApiKeyLayer`
- `require_scope()` returns `Error::forbidden()`
- Expired/revoked keys fail verification immediately
- No new dependencies — uses existing `encoding`, `rand`, `subtle`
- Scopes stored as `Vec<String>` — framework stores and extracts, app defines meaning (mirrors RBAC philosophy)

---

## 3. Tier-Based Gating (`tier`)

**Purpose:** Plan-based feature gating for SaaS apps. Resolves the current tenant's plan and gates access to features and usage limits.

**Feature flag:** None — always available.

### Public API

```rust
// Backend trait
pub trait TierBackend: Send + Sync {
    fn resolve(&self, tenant_id: &str) -> Pin<Box<dyn Future<Output = Result<TierInfo>> + Send + '_>>;
}

// Concrete wrapper
#[derive(Clone)]
pub struct TierResolver(Arc<dyn TierBackend>);

impl TierResolver {
    /// App provides its own resolution logic (no built-in default).
    pub fn from_backend(backend: Arc<dyn TierBackend>) -> Self;

    pub async fn resolve(&self, tenant_id: &str) -> Result<TierInfo>;
}

// Unified feature model
#[derive(Debug, Clone, Serialize)]
pub enum FeatureAccess {
    /// Feature is enabled or disabled (boolean gate).
    Toggle(bool),
    /// Feature has a usage limit ceiling.
    Limit(u64),
}

#[derive(Debug, Clone, Serialize)]
pub struct TierInfo {
    pub name: String,                             // "free", "starter", "pro"
    pub features: HashMap<String, FeatureAccess>, // unified feature map
}

impl TierInfo {
    /// Check if a feature is available (Toggle=true or Limit>0).
    pub fn has_feature(&self, name: &str) -> bool;

    /// Check if feature is explicitly enabled (Toggle only).
    pub fn is_enabled(&self, name: &str) -> bool;

    /// Get the limit ceiling for a feature (Limit only).
    pub fn limit(&self, name: &str) -> Option<u64>;
}
```

### Middleware & Guards

```rust
// Resolves tier from tenant, stores in request extensions
pub struct TierLayer { .. }
impl TierLayer {
    pub fn new(resolver: TierResolver) -> Self;
}

// Guards — return Error::forbidden(), app's error handler renders
pub fn require_feature(name: &str) -> impl Layer;
// returns Error::forbidden("Feature 'custom_domain' is not available on your current plan")

pub fn require_limit(name: &str, count: u64) -> impl Layer;
// returns Error::forbidden("Limit exceeded for 'api_calls'")
```

### Usage

```rust
// --- App defines its own resolution ---
struct MyTierBackend { db: Database }

impl TierBackend for MyTierBackend {
    fn resolve(&self, tenant_id: &str) -> Pin<Box<dyn Future<Output = Result<TierInfo>> + Send + '_>> {
        Box::pin(async move {
            let plan: String = self.db.conn()
                .query_one_map(
                    "SELECT plan FROM tenants WHERE id = ?1",
                    libsql::params![tenant_id],
                    |row| { /* ... */ },
                ).await?;

            Ok(match plan.as_str() {
                "free" => TierInfo {
                    name: "free".into(),
                    features: HashMap::from([
                        ("basic_export".into(), FeatureAccess::Toggle(true)),
                        ("sso".into(), FeatureAccess::Toggle(false)),
                        ("api_calls".into(), FeatureAccess::Limit(1_000)),
                    ]),
                },
                "pro" => TierInfo {
                    name: "pro".into(),
                    features: HashMap::from([
                        ("basic_export".into(), FeatureAccess::Toggle(true)),
                        ("custom_domain".into(), FeatureAccess::Toggle(true)),
                        ("sso".into(), FeatureAccess::Toggle(true)),
                        ("api_calls".into(), FeatureAccess::Limit(100_000)),
                        ("storage_mb".into(), FeatureAccess::Limit(5_000)),
                    ]),
                },
                _ => return Err(Error::not_found()),
            })
        })
    }
}

// --- Handler using extractor ---
async fn domain_settings(
    tier: TierInfo,
) -> Result<Response> {
    // tier.name, tier.has_feature("sso"), tier.limit("api_calls")
}

// --- Wiring ---
let resolver = TierResolver::from_backend(Arc::new(MyTierBackend { db: db.clone() }));

let app = Router::new()
    .route("/settings/domain", get(domain_settings))
    .route_layer(require_feature("custom_domain"))
    .layer(TierLayer::new(resolver));
```

### Design Notes

- No `TierResolver::new(db)` — the framework provides the trait and middleware, the app owns the mapping logic (hardcoded, config-based, or DB-driven)
- `TierLayer` requires `TenantLayer` to run first (needs `TenantId` in extensions)
- Guards return `Error::forbidden()` — app's error handler decides rendering
- `require_limit()` compares against a count the app provides — the tier module doesn't track usage, it only knows the limit ceiling
- `TierInfo` extractor panics if `TierLayer` is missing — same pattern as `Session` extractor
- `TierInfo` is cached per-request (resolved once by middleware, read by multiple guards/handlers)
- Each feature is either a `Toggle(bool)` or a `Limit(u64)` — unified model, no separate `features` + `limits` collections

---

## 4. Embeddings (`embed`)

**Purpose:** Text-to-vector via LLM provider APIs. Ships with OpenAI, Mistral, and Vertex AI providers. Custom providers via trait.

**Feature flag:** `embed` — depends on `http-client` (already exists). No new dependencies.

### Public API

```rust
// Backend trait — returns f32 blobs ready for libsql F32_BLOB columns
pub trait EmbeddingBackend: Send + Sync {
    fn embed(&self, input: &[&str]) -> Pin<Box<dyn Future<Output = Result<Vec<Vec<u8>>>> + Send + '_>>;
    fn dimensions(&self) -> usize;
    fn model_name(&self) -> &str;
}

// Concrete wrapper
#[derive(Clone)]
pub struct EmbeddingProvider(Arc<dyn EmbeddingBackend>);

impl EmbeddingProvider {
    pub fn from_backend(backend: Arc<dyn EmbeddingBackend>) -> Self;

    /// Embed a batch of texts. Returns one f32 blob per input.
    pub async fn embed(&self, input: &[&str]) -> Result<Vec<Vec<u8>>>;

    /// Embed a single text. Convenience wrapper.
    pub async fn embed_one(&self, input: &str) -> Result<Vec<u8>>;

    /// Vector dimensions for this provider/model.
    pub fn dimensions(&self) -> usize;

    /// Model name string.
    pub fn model_name(&self) -> &str;
}

// Vector conversion helpers
/// Encode floats to little-endian F32_BLOB (for custom backends).
pub fn to_f32_blob(v: &[f32]) -> Vec<u8>;

/// Decode a F32_BLOB column value back to floats.
pub fn from_f32_blob(blob: &[u8]) -> Vec<f32>;
```

### Providers

Each provider is a standalone struct implementing `EmbeddingBackend`:

```rust
// OpenAI
pub struct OpenAIEmbedding { .. } // Arc<Inner>
impl OpenAIEmbedding {
    pub fn new(client: http::Client, config: &OpenAIConfig) -> Self;
}
impl EmbeddingBackend for OpenAIEmbedding { .. }

// Mistral
pub struct MistralEmbedding { .. }
impl MistralEmbedding {
    pub fn new(client: http::Client, config: &MistralConfig) -> Self;
}
impl EmbeddingBackend for MistralEmbedding { .. }

// Vertex AI
pub struct VertexEmbedding { .. }
impl VertexEmbedding {
    pub fn new(client: http::Client, config: &VertexConfig) -> Self;
}
impl EmbeddingBackend for VertexEmbedding { .. }
```

### Provider Configs

```rust
pub struct OpenAIConfig {
    pub api_key: String,
    pub model: String,            // default: "text-embedding-3-small"
    pub dimensions: usize,        // default: 1536
    pub base_url: Option<String>, // override for proxies/compatible APIs (Azure OpenAI, etc.)
}

pub struct MistralConfig {
    pub api_key: String,
    pub model: String,            // default: "mistral-embed"
    pub dimensions: usize,        // default: 1024
}

pub struct VertexConfig {
    pub project_id: String,
    pub location: String,         // default: "us-central1"
    pub model: String,            // default: "text-embedding-005"
    pub dimensions: usize,        // default: 768
    pub access_token: String,     // short-lived OAuth2 token, app manages refresh
}
```

### YAML Configuration

```yaml
embed:
    provider: openai
    openai:
        api_key: "${OPENAI_API_KEY}"
        model: "text-embedding-3-small"
        dimensions: 1536
    mistral:
        api_key: "${MISTRAL_API_KEY}"
        model: "mistral-embed"
        dimensions: 1024
    vertex:
        project_id: "${GCP_PROJECT_ID}"
        location: "us-central1"
        model: "text-embedding-005"
        dimensions: 768
        access_token: "${GCP_ACCESS_TOKEN}"
```

### Usage

```rust
// --- Embed and store in libsql vector column ---

async fn index_document(
    Service(embedder): Service<EmbeddingProvider>,
    Service(db): Service<Database>,
    body: JsonRequest<Document>,
) -> Result<()> {
    let embedding = embedder.embed_one(&body.content).await?;

    db.conn().execute_raw(
        "INSERT INTO documents (id, content, embedding) VALUES (?1, ?2, ?3)",
        libsql::params![id::ulid(), body.content.as_str(), embedding],
    ).await?;

    Ok(())
}

// --- Similarity search using libsql vector_top_k ---

async fn search(
    Service(embedder): Service<EmbeddingProvider>,
    Service(db): Service<Database>,
    Query(q): Query<SearchQuery>,
) -> Result<Json<Vec<SearchResult>>> {
    let query_vec = embedder.embed_one(&q.text).await?;

    let results: Vec<SearchResult> = db.conn().query_all(
        "SELECT d.id, d.content \
         FROM vector_top_k('documents_idx', ?1, ?2) AS v \
         JOIN documents AS d ON d.rowid = v.id",
        libsql::params![query_vec, q.limit.unwrap_or(10)],
    ).await?;

    Ok(Json(results))
}

// --- Wiring ---

let http_client = http::Client::new();
let embedder = EmbeddingProvider::from_backend(
    Arc::new(OpenAIEmbedding::new(http_client, &openai_config))
);

let app = Router::new()
    .route("/api/documents", post(index_document))
    .route("/api/search", get(search))
    .with_service(embedder);
```

### Design Notes

- Backend returns `Vec<u8>` (little-endian f32 blob) — ready for libsql `F32_BLOB` columns, no serialization step in handlers
- Internally each provider: calls API -> gets `Vec<f32>` -> converts to LE blob before returning
- `to_f32_blob()` / `from_f32_blob()` exposed for custom backends and reading vectors back from DB
- `embed()` takes a batch — all three APIs support batch embedding in a single HTTP call
- `embed_one()` is a convenience that calls `embed(&[input])` and returns the first result
- OpenAI `base_url` override allows using compatible APIs (Azure OpenAI, local proxies)
- Vertex requires a short-lived OAuth2 token — the app is responsible for refreshing it
- No retry logic in the module — the underlying `http::Client` handles retries per its config
- No caching — app can wrap `EmbeddingProvider` or cache at the DB level
- libsql auto-detects even-length blobs as F32_BLOB — no `vector()` SQL function needed

---

## 5. Data Export (`export`)

**Purpose:** Stream query results into downloadable file formats (CSV, JSON Lines). Handles Content-Type, Content-Disposition headers, and streaming large result sets without buffering.

**Feature flag:** `export` — pulls in `csv` crate.

### Public API

```rust
// Column mapping — controls what gets exported and display names
pub struct ColumnMap { .. }

impl ColumnMap {
    pub fn new() -> Self;
    pub fn column(self, field: &str, display: &str) -> Self;  // "email" → "Email Address"
}

// CSV export
pub struct CsvExport { .. }

impl CsvExport {
    pub fn new(columns: ColumnMap) -> Self;
    pub fn filename(self, name: &str) -> Self;
    pub fn from_stream<T: Serialize>(self, stream: impl Stream<Item = T>) -> Self;
    pub fn into_response(self) -> Response;
}

// JSON Lines export
pub struct JsonLinesExport { .. }

impl JsonLinesExport {
    pub fn new() -> Self;
    pub fn filename(self, name: &str) -> Self;
    pub fn from_stream<T: Serialize>(self, stream: impl Stream<Item = T>) -> Self;
    pub fn into_response(self) -> Response;
}
```

### Response Headers

```
Content-Type: text/csv; charset=utf-8              (CSV)
Content-Type: application/x-ndjson                 (JSON Lines)
Content-Disposition: attachment; filename="users.csv"
Transfer-Encoding: chunked
```

### Usage

```rust
async fn export_users(
    filter: Filter,
    Service(db): Service<Database>,
) -> Result<Response> {
    let schema = FilterSchema::new()
        .field("status", FieldType::Text)
        .field("created_at", FieldType::Date)
        .sortable(&["created_at"]);

    let validated = filter.validate(&schema)?;

    let stream = db.conn()
        .select("SELECT id, email, name, created_at FROM users")
        .filter(validated)
        .stream::<UserRow>()
        .await?;

    let columns = ColumnMap::new()
        .column("id", "ID")
        .column("email", "Email Address")
        .column("name", "Full Name")
        .column("created_at", "Created");

    Ok(CsvExport::new(columns)
        .filename("users-export.csv")
        .from_stream(stream)
        .into_response())
}
```

### Design Notes

- Streaming: rows are serialized and flushed in chunks — no full result set in memory
- `ColumnMap` controls what is exported and the display names — prevents accidentally exporting sensitive fields (password hashes, tokens)
- `from_stream()` takes any `Stream<Item = T>` where `T: Serialize` — decoupled from database
- JSON Lines (one JSON object per line) is preferred over a JSON array for streaming — the consumer can parse line-by-line without buffering
- CSV serialization via the `csv` crate (handles escaping correctly)
- No Excel/PDF — CSV covers 90% of export needs, consumers can open CSV in Excel
- **Prerequisite:** `SelectBuilder::stream()` method needs to be added to `src/db/select.rs` — returns a `Stream<Item = T>` over query results without collecting into `Vec`. This is a small addition to the existing db module, not a new module.

---

## Cross-Cutting Concerns

### Error Handling

All modules use `modo::Error` and `modo::Result<T>`. All middleware and guards return `Error` — the app's error handler decides rendering.

- `audit`: `record()` errors are traced but optionally swallowed (configurable)
- `apikey`: `Error::unauthorized()` for invalid/expired/revoked keys, `Error::forbidden()` for missing scopes
- `tier`: `Error::forbidden()` for plan-gated features and limit violations
- `embed`: `Error::internal()` for provider API failures, `Error::bad_request()` for empty input
- `export`: `Error::internal()` for serialization failures

### Testing

Each module provides:

- Unit tests for core logic (builders, parsers, evaluation, key generation)
- Integration tests using `TestApp` and `TestDb` where DB is involved
- Test helpers where useful (e.g., `audit::test::in_memory()`, `apikey::test::mock_store()`, `tier::test::free_tier()`)

### Configuration

Modules that read from YAML follow modo's existing `Config` pattern with `${VAR}` substitution. New config sections nest under their module name (`apikey:`, `embed:`, etc.).
