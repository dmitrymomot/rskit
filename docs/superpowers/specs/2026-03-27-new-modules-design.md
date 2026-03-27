# New Modules Design — 2026-03-27

High-level design for 8 new modo framework modules: HTTP client, pagination, search/filtering, audit logging, data export, feature flags, tier-based gating, and domain-verified signup.

## Build Phases

```
Phase 1 (foundation):
  http — unblocks refactor of storage, webhook, auth::oauth

Phase 2 (data layer, parallel):
  page   — standalone
  audit  — standalone
  flag   — standalone

Phase 3 (builds on phase 2):
  filter        — composes with page
  tier          — independent, designed after flag for clean boundary
  domain_signup — independent

Phase 4:
  export — composes with filter + page
```

## Dependency Map

```
http ← standalone, no deps on new modules
  ↑ refactor: storage, webhook, auth::oauth switch to this

filter → page (filter composes before paginate)
  ↓
export (export a filtered, paginated stream)

audit  ← standalone, uses db pool
flag   ← standalone, config + optional db
tier   ← standalone, works with tenant module
domain_signup ← standalone, uses dns + tenant + db
```

---

## 1. HTTP Client (`http`)

**Purpose:** Ergonomic async HTTP client built on hyper/hyper-rustls. Single client used across the entire framework, replacing per-module HTTP implementations in `storage`, `webhook`, and `auth::oauth`.

**Feature flag:** `http-client` — pulls in hyper, hyper-rustls, hyper-util, http-body-util. Currently these deps are gated behind `auth`, `storage`, `webhooks`. The new `http` module consolidates them under one feature. Modules that need HTTP (`auth`, `storage`, `webhooks`) depend on `http-client` in their feature definitions. Apps that don't use any HTTP-dependent module pay no cost.

### Public API

```rust
// Construction
Client::new() -> Client                    // defaults
Client::from_config(config: &ClientConfig) -> Client
Client::builder() -> ClientBuilder         // fine-grained

// Request methods — each returns RequestBuilder
client.get(url)
client.post(url)
client.put(url)
client.patch(url)
client.delete(url)
client.request(method, url)               // escape hatch

// RequestBuilder
.header(name, value)
.headers(HeaderMap)
.bearer_token(token)
.basic_auth(user, pass)
.query(&[(k, v)])                          // append query params
.json(&body)                               // serialize + Content-Type
.form(&body)                               // url-encoded + Content-Type
.body(bytes)                               // raw body
.timeout(Duration)                         // per-request override
.send() -> Result<Response>

// Response
.status() -> StatusCode
.headers() -> &HeaderMap
.json<T: DeserializeOwned>() -> Result<T>
.text() -> Result<String>
.bytes() -> Result<Bytes>
.stream() -> impl Stream<Item = Result<Bytes>>  // for large responses
.content_length() -> Option<u64>
```

### Configuration (YAML)

```yaml
http:
  timeout_secs: 30           # default request timeout
  connect_timeout_secs: 5    # TCP connect timeout
  user_agent: "modo/0.1"     # default User-Agent header
  max_retries: 0             # retry count (0 = no retries)
  retry_backoff_ms: 100      # initial backoff between retries
```

### Retry Policy

- Retries on: connection errors, 502, 503, 429 (with Retry-After respect)
- Does NOT retry: 4xx (except 429), 5xx other than 502/503, request body already consumed
- Exponential backoff: `retry_backoff_ms * 2^attempt`
- Max retries capped at config value

### Consolidation Plan

After the client module exists:

1. `webhook::HyperClient` → uses `http::Client` internally, `HttpClient` trait stays for testability
2. `storage` internal HTTP calls → replaced with `http::Client`
3. `auth::oauth` token exchange + userinfo fetch → replaced with `http::Client`
4. Remove duplicated hyper/connection-pool setup from each module

The `HttpClient` trait in `webhook` may still be useful for mocking in tests. The concrete `HyperClient` impl becomes a thin wrapper over `http::Client`.

### Design Notes

- `Client` is cheaply cloneable (`Arc<Inner>` pattern per modo convention)
- Connection pooling via hyper's built-in pool (shared across all framework modules)
- TLS via hyper-rustls with webpki roots (already in dep tree)
- No new dependencies required

---

## 2. Pagination (`page`)

**Purpose:** Offset-based and cursor-based pagination with typed result containers and request extractors.

**Feature flag:** None — always available.

### Public API

**Result types:**

```rust
// Offset pagination
Page<T> {
    items: Vec<T>,
    total: u64,            // total matching rows
    page: u32,             // current page (1-based)
    per_page: u32,         // items per page
    total_pages: u32,      // ceil(total / per_page)
    has_next: bool,
    has_prev: bool,
}

// Cursor pagination
CursorPage<T> {
    items: Vec<T>,
    next_cursor: Option<String>,   // opaque, base64url-encoded
    prev_cursor: Option<String>,
    has_more: bool,
    per_page: u32,
}
```

Both implement `Serialize` for consistent JSON API responses.

**Request extractors:**

```rust
// Offset — from ?page=2&per_page=20
PageRequest {
    page: u32,       // default 1, min 1
    per_page: u32,   // default 20, min 1, max configurable (default 100)
}

// Cursor — from ?cursor=abc&per_page=20
CursorRequest {
    cursor: Option<String>,   // None = first page
    per_page: u32,            // default 20, min 1, max configurable
    direction: Direction,     // Forward (default) or Backward
}

enum Direction { Forward, Backward }
```

**Query helpers:**

```rust
// Offset pagination — appends COUNT(*) query + LIMIT/OFFSET
paginate<T>(
    sql: &str,
    args: SqliteArguments,
    request: &PageRequest,
    pool: &Pool,    // or &ReadPool
) -> Result<Page<T>>

// Cursor pagination — appends WHERE + ORDER BY + LIMIT
paginate_cursor<T>(
    sql: &str,
    args: SqliteArguments,
    cursor_column: &str,          // column to paginate on (must be unique + ordered)
    request: &CursorRequest,
    pool: &Pool,
) -> Result<CursorPage<T>>
```

### Cursor Encoding

Cursor is an opaque base64url string encoding `(column_value, id)`. The consumer never constructs or parses cursors — they pass them through from response to next request.

Internal format: `base64url(json({"v": column_value, "id": row_id}))`.

Using `id` as a tiebreaker handles non-unique sort columns (e.g., `created_at` with duplicate timestamps).

### Configuration

```yaml
pagination:
  default_per_page: 20
  max_per_page: 100
```

### Design Notes

- Pages are 1-based (not 0-based) — matches user expectations and most API conventions
- `per_page` is clamped to `[1, max_per_page]` silently — no error on out-of-range
- Offset pagination runs two queries: `SELECT COUNT(*)` + `SELECT ... LIMIT/OFFSET`. For SQLite this is fine at moderate scale
- Cursor pagination runs one query: `SELECT ... WHERE cursor_col > ? ORDER BY cursor_col LIMIT per_page + 1`. The +1 trick detects `has_more`
- Both helpers take raw SQL strings — no query builder dependency. Composes with the filter module when available

---

## 3. Search/Filtering (`filter`)

**Purpose:** Composable query builder for WHERE clauses + optional request-level DSL for API filtering.

**Feature flag:** None — always available.

### Builder Layer

```rust
FilterBuilder::new()
    .eq("status", "active")
    .neq("role", "admin")
    .gt("created_at", "2026-01-01")
    .gte("age", 18)
    .lt("score", 100)
    .lte("priority", 5)
    .like("name", "%john%")
    .ilike("email", "%@acme.com")       // case-insensitive LIKE
    .in_list("status", &["active", "pending"])
    .not_in("role", &["banned", "suspended"])
    .between("age", 18, 65)
    .is_null("deleted_at")
    .is_not_null("verified_at")
    .or(|f| f.eq("role", "admin").eq("role", "owner"))  // OR group
    .sort("created_at", Desc)
    .sort("name", Asc)
    .to_sql() -> (String, Vec<SqliteArgument>)
```

`to_sql()` returns a fragment like `WHERE status = ? AND age >= ? ORDER BY created_at DESC` with bound parameters. This fragment is appended to a base query by the caller or by pagination helpers.

### Request Parser Layer

Parses query string into a `FilterBuilder`:

```
GET /api/users?filter=name:like:john,status:in:active|pending&sort=-created_at,name
```

**Filter syntax:** `field:op:value` comma-separated.

| Operator | Meaning | Example |
|----------|---------|---------|
| `eq` | equals | `status:eq:active` |
| `neq` | not equals | `role:neq:admin` |
| `gt` | greater than | `age:gt:18` |
| `gte` | greater or equal | `age:gte:18` |
| `lt` | less than | `score:lt:100` |
| `lte` | less or equal | `priority:lte:5` |
| `like` | SQL LIKE | `name:like:%john%` |
| `in` | IN list | `status:in:active\|pending` |
| `null` | IS NULL | `deleted_at:null` |
| `notnull` | IS NOT NULL | `verified_at:notnull` |

**Sort syntax:** comma-separated field names, prefix `-` for descending. `sort=-created_at,name` → `ORDER BY created_at DESC, name ASC`.

### FilterRequest Extractor

```rust
// Define allowed fields + their SQL types
let schema = FilterSchema::new()
    .field("name", SqlType::Text)
    .field("status", SqlType::Text)
    .field("created_at", SqlType::DateTime)
    .field("age", SqlType::Integer)
    .sortable(&["name", "created_at", "age"]);  // only these can be sorted

// Extractor in handler
async fn list_users(
    filter: FilterRequest,       // parsed from query string
    page: PageRequest,           // from pagination module
    Service(pool): Service<ReadPool>,
) -> Result<Json<Page<User>>> {
    let builder = filter.to_builder(&schema)?;  // validates against schema
    let (where_clause, args) = builder.to_sql();
    let sql = format!("SELECT * FROM users {where_clause}");
    let result = paginate(&sql, args, &page, &pool).await?;
    Ok(Json(result))
}
```

### Design Notes

- `FilterSchema` validates that only declared fields are filterable — prevents SQL injection via column names and filtering on unindexed columns
- Field names are validated against an allowlist — the actual column name in SQL is controlled by the schema, not the request
- Values are always bound as parameters — no string interpolation
- `ilike` appends `COLLATE NOCASE` explicitly — SQLite's `LIKE` is case-insensitive for ASCII by default, but `COLLATE NOCASE` makes it explicit and consistent
- The builder and parser are independent — you can use the builder without the request parser, or write a custom parser that produces a `FilterBuilder`
- `or()` groups produce `(condition OR condition)` with parentheses

---

## 4. Audit Logging (`audit`)

**Purpose:** Explicit event logging for business-significant actions. SQLite-backed with a universal schema that works for both single-tenant and multi-tenant apps.

**Feature flag:** None — always available.

### Public API

```rust
// Recording events
AuditLog trait {
    async fn record(&self, entry: &AuditEntry) -> Result<()>;
}

AuditEntry {
    actor: String,              // who: user ID, "system", API key ID
    action: String,             // what: "user.role.changed", "account.deleted"
    resource_type: String,      // on what kind: "user", "tenant", "api_key"
    resource_id: String,        // on which one: "usr_01ABC..."
    metadata: serde_json::Value,// extra context: {"old_role": "editor", "new_role": "admin"}
    ip: Option<String>,         // client IP if available
    tenant_id: Option<String>,  // NULL for single-tenant apps, set for multi-tenant
}

// Implementation
SqliteAuditLog::new(pool: Pool) -> SqliteAuditLog
```

### Repository (Query Interface)

```rust
AuditRepo::new(pool: Pool) -> AuditRepo  // or ReadPool

// Query methods — all return paginated results
repo.list(&PageRequest) -> Result<Page<AuditRecord>>
repo.by_actor(actor: &str, &PageRequest) -> Result<Page<AuditRecord>>
repo.by_resource(resource_type: &str, resource_id: &str, &PageRequest) -> Result<Page<AuditRecord>>
repo.by_tenant(tenant_id: &str, &PageRequest) -> Result<Page<AuditRecord>>
repo.by_action(action: &str, &PageRequest) -> Result<Page<AuditRecord>>

// Composes with filter module
repo.query(filter: &FilterBuilder, &PageRequest) -> Result<Page<AuditRecord>>
```

`AuditRecord` is the stored form: all `AuditEntry` fields plus `id` (ULID) and `created_at` (timestamp).

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

### Universal Schema Design

The schema works for both single-tenant and multi-tenant apps:

- **Single-tenant apps:** `tenant_id` is always `NULL`. The column exists but is ignored. Queries like `by_actor()` work without filtering on tenant. The partial index on `tenant_id` has zero overhead when all values are NULL.
- **Multi-tenant apps:** `tenant_id` is set on every entry. `by_tenant()` filters by it. The `AuditLog` middleware/helper can auto-populate `tenant_id` from the resolved `TenantId` in request extensions.

### Handler Usage

```rust
async fn change_role(
    session: Session,
    Service(audit): Service<Arc<dyn AuditLog>>,
    // ...
) -> Result<()> {
    // ... perform the action ...

    audit.record(&AuditEntry {
        actor: session.user_id(),
        action: "user.role.changed".into(),
        resource_type: "user".into(),
        resource_id: target_user_id.into(),
        metadata: serde_json::json!({"old_role": old, "new_role": new}),
        ip: None,  // or from ClientIp extractor
        tenant_id: None,  // or from Tenant extractor
    }).await?;
    Ok(())
}
```

### Design Notes

- `AuditLog` is a trait behind `Arc<dyn AuditLog>` — testable with in-memory impls
- `record()` is fire-and-forget from the handler's perspective — errors are logged but don't fail the request (configurable)
- Actions use dot-notation by convention: `resource.verb` or `resource.sub.verb`
- `metadata` is unstructured JSON — keeps the schema stable while allowing arbitrary context
- No automatic middleware capture — handlers explicitly log what matters

---

## 5. Data Export (`export`)

**Purpose:** Stream query results into downloadable file formats (CSV, JSON Lines). Handles Content-Type, Content-Disposition headers, and streaming large result sets without buffering.

**Feature flag:** `export` — pulls in `csv` crate. The `ExportQuery` convenience helper composes with the `filter` module (always available) and `page` module (always available), so no cross-feature dependency issues.

### Public API

```rust
// Column mapping
ColumnMap::new()
    .column("id", "ID")                    // field name → display name
    .column("email", "Email Address")
    .column("created_at", "Created")
    .computed("full_name", |row| format!("{} {}", row.first, row.last))

// CSV export
CsvExport::new(columns: ColumnMap)
    .filename("users-export.csv")          // Content-Disposition filename
    .from_stream(stream)                   // any Stream<Item = T: Serialize>
    .into_response() -> Response           // axum response, streaming

// JSON Lines export
JsonLinesExport::new()
    .filename("users-export.jsonl")
    .from_stream(stream)
    .into_response() -> Response

// Convenience: export from a filtered, paginated query
ExportQuery::new("SELECT * FROM users", args)
    .filter(filter_builder)               // optional: compose with filter module
    .pool(&pool)
    .stream()                             // returns Stream<Item = Row>
```

### Response Headers

```
Content-Type: text/csv; charset=utf-8           (CSV)
Content-Type: application/x-ndjson              (JSON Lines)
Content-Disposition: attachment; filename="users-export.csv"
Transfer-Encoding: chunked
```

### Design Notes

- Streaming: rows are serialized and flushed in chunks — no full result set in memory
- `ColumnMap` controls what is exported and the display names — prevents accidentally exporting sensitive fields (password hashes, tokens)
- `from_stream()` takes any `Stream<Item = T>` where `T: Serialize` — decoupled from database
- JSON Lines (one JSON object per line) is preferred over a JSON array for streaming — the consumer can parse line-by-line without buffering
- `ExportQuery` is a convenience that combines raw SQL with the filter module and streams results via sqlx's row streaming
- CSV serialization via the `csv` crate (well-maintained, handles escaping correctly)
- No Excel/PDF — those require heavyweight deps. CSV covers 90% of export needs, consumers can open CSV in Excel

---

## 6. Feature Flags (`flag`)

**Purpose:** Operational toggles for gradual rollout and kill switches. Config-based by default with optional SQLite-backed runtime source.

**Feature flag:** None — always available.

### Public API

```rust
// Source trait
FlagSource trait {
    async fn is_enabled(&self, flag: &str, ctx: &FlagContext) -> bool;
    async fn all_flags(&self) -> Result<Vec<FlagDefinition>>;
}

// Context for evaluation
FlagContext {
    tenant_id: Option<String>,
    user_id: Option<String>,
    // percentage bucketing derived from tenant_id or user_id
}

// Flag definition
FlagDefinition {
    name: String,
    enabled: bool,               // global default
    rollout_percentage: Option<u8>,  // 0-100, None = use enabled field
    overrides: Vec<FlagOverride>,
}

FlagOverride {
    tenant_id: Option<String>,   // if set, override for this tenant
    user_id: Option<String>,     // if set, override for this user
    enabled: bool,
}
```

### Sources

**ConfigSource** — reads from YAML config at startup:

```yaml
flags:
  new_dashboard:
    enabled: false
    rollout_percentage: 10      # 10% of users
  beta_export:
    enabled: true
    overrides:
      - tenant_id: "tenant_abc"
        enabled: false          # disabled for this tenant
```

**SqliteSource** — reads from DB table, cached with TTL:

```rust
SqliteSource::new(pool: Pool, cache_ttl: Duration) -> SqliteSource
```

Recommended schema (documented, migration owned by end app):

```sql
CREATE TABLE feature_flags (
    name       TEXT PRIMARY KEY,
    enabled    INTEGER NOT NULL DEFAULT 0,    -- 0/1
    rollout_pct INTEGER,                      -- 0-100, NULL = use enabled
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE TABLE feature_flag_overrides (
    id         TEXT PRIMARY KEY,              -- ULID
    flag_name  TEXT NOT NULL REFERENCES feature_flags(name),
    tenant_id  TEXT,
    user_id    TEXT,
    enabled    INTEGER NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE UNIQUE INDEX idx_flag_override_unique
    ON feature_flag_overrides(flag_name, tenant_id, user_id);
```

### Evaluation Logic

1. Check overrides: exact `(flag, tenant_id, user_id)` match → use it
2. Check overrides: `(flag, tenant_id, NULL)` match → use it
3. Check `rollout_percentage`: hash `tenant_id` or `user_id` to a 0-99 bucket, enable if bucket < percentage
4. Fall back to `enabled` field

### Handler Usage

```rust
// Extractor
async fn dashboard(
    flag: Flag,   // extractor, auto-populates FlagContext from session/tenant
) -> Result<Response> {
    if flag.is_enabled("new_dashboard") {
        // render new dashboard
    } else {
        // render old dashboard
    }
}

// Guard middleware
Router::new()
    .route("/beta/export", get(export_handler))
    .route_layer(flag::require("beta_export"))  // 404 if flag is off
```

### Design Notes

- `Flag` extractor builds `FlagContext` automatically from `Session` and `Tenant<T>` extensions if present
- `FlagSource` is `Arc<dyn FlagSource>` — pluggable, testable
- `ConfigSource` is evaluated in-memory (no async), loaded once at startup
- `SqliteSource` caches flag state in-memory with configurable TTL to avoid per-request DB queries
- Percentage rollout uses a deterministic hash of the context identifier — same user always gets the same result
- `require()` guard returns 404 (not 403) — the route doesn't exist for users without the flag
- Unknown flags evaluate to `false` — safe default

---

## 7. Tier (`tier`)

**Purpose:** Plan-based feature gating for SaaS apps. Resolves the current tenant's plan and gates access to features and usage limits.

**Feature flag:** None — always available.

### Public API

```rust
// Resolution trait
TierResolver trait {
    async fn resolve(&self, tenant_id: &str) -> Result<TierInfo>;
}

// Resolved tier info
TierInfo {
    name: String,                        // "free", "starter", "pro"
    features: HashSet<String>,           // "sso", "custom_domain", "export"
    limits: HashMap<String, u64>,        // "api_calls" -> 1000, "storage_mb" -> 500
}

// Extractor
Tier  // available in handlers after TierLayer middleware

tier.name() -> &str
tier.has_feature(name: &str) -> bool
tier.limit(name: &str) -> Option<u64>
```

### Middleware & Guards

```rust
// Middleware: resolves tier from tenant, stores in request extensions
TierLayer::new(resolver: Arc<dyn TierResolver>)

// Guard: requires feature in the current tier
tier::require("custom_domain")           // returns 403 if plan lacks feature
tier::require_limit("api_calls", count)  // returns 403 if count exceeds limit

// Router wiring
Router::new()
    .route("/settings/domain", get(domain_settings))
    .route_layer(tier::require("custom_domain"))
    .layer(TierLayer::new(resolver))
```

### App-Defined Resolution

The framework provides the trait and middleware. The app implements `TierResolver` with its own plan-to-features mapping:

```rust
// Example: hardcoded mapping
struct MyTierResolver { pool: ReadPool }

impl TierResolver for MyTierResolver {
    async fn resolve(&self, tenant_id: &str) -> Result<TierInfo> {
        let plan = sqlx::query_scalar("SELECT plan FROM tenants WHERE id = ?")
            .bind(tenant_id)
            .fetch_one(&*self.pool)
            .await?;

        Ok(match plan.as_str() {
            "free" => TierInfo {
                name: "free".into(),
                features: HashSet::from(["basic_export".into()]),
                limits: HashMap::from([("api_calls".into(), 1000)]),
            },
            "pro" => TierInfo {
                name: "pro".into(),
                features: HashSet::from(["basic_export".into(), "custom_domain".into(), "sso".into()]),
                limits: HashMap::from([("api_calls".into(), 100_000)]),
            },
            _ => return Err(Error::not_found()),
        })
    }
}
```

### Design Notes

- `TierResolver` is `Arc<dyn TierResolver>` — app owns the mapping logic (hardcoded, config-based, or DB-driven)
- `TierLayer` requires `TenantLayer` to run first (needs `TenantId` in extensions)
- `require()` returns 403 Forbidden with a message like `"Feature 'custom_domain' is not available on your current plan"`
- `require_limit()` compares against a current count the app provides — the tier module doesn't track usage, it only knows the limit ceiling
- `Tier` extractor panics if `TierLayer` is missing — same pattern as `Session` extractor
- `TierInfo` is cached per-request (resolved once by middleware, read by multiple guards/handlers)
- Distinction from `flag` module: flags are operational (rollout toggles, kill switches) and temporary. Tiers are product/business (plan features) and permanent. They can coexist — a feature might require both a flag (is it rolled out?) and a tier (does the plan include it?)

---

## 8. Domain-Verified Signup (`domain_signup`)

**Purpose:** Allow tenants to claim email domains so that users with matching verified email addresses auto-join the tenant. Enables "everyone at @acme.com can sign in" without SSO/IdP integration.

**Feature flag:** None — always available. Uses the existing `dns` module (feature-gated) for domain verification, but the registry and lookup logic work independently.

### Public API

```rust
// Registry trait
DomainRegistry trait {
    /// Register a domain claim for a tenant. Returns a verification token.
    async fn register(&self, tenant_id: &str, domain: &str) -> Result<DomainClaim>;

    /// Check verification status (calls DNS verifier if pending).
    async fn verify(&self, tenant_id: &str, domain: &str) -> Result<DomainStatus>;

    /// Remove a domain claim.
    async fn remove(&self, tenant_id: &str, domain: &str) -> Result<()>;

    /// Look up which tenant owns a verified domain.
    async fn lookup_domain(&self, domain: &str) -> Result<Option<TenantMatch>>;

    /// Look up tenant for an email address (extracts domain, finds match).
    async fn lookup_email(&self, email: &str) -> Result<Option<TenantMatch>>;

    /// List all domains for a tenant.
    async fn list(&self, tenant_id: &str) -> Result<Vec<DomainClaim>>;
}

// Types
DomainClaim {
    tenant_id: String,
    domain: String,
    verification_token: String,      // TXT record value to set
    status: DomainStatus,
    created_at: chrono::DateTime<Utc>,
    verified_at: Option<chrono::DateTime<Utc>>,
}

enum DomainStatus {
    Pending,       // registered, awaiting DNS verification
    Verified,      // TXT record confirmed
    Failed,        // verification attempted, DNS check failed
}

TenantMatch {
    tenant_id: String,
    domain: String,
}
```

### Implementation

```rust
SqliteDomainRegistry::new(pool: Pool, dns_verifier: Option<Arc<DomainVerifier>>) -> Self
```

When `dns_verifier` is `Some`, `verify()` performs a live DNS TXT record lookup using the existing `dns::DomainVerifier`. When `None`, verification must be done externally (manual approval, API call from admin).

### Recommended Schema

Documented in module docs. Migration owned by end app.

```sql
CREATE TABLE tenant_domains (
    tenant_id          TEXT NOT NULL,
    domain             TEXT NOT NULL,
    verification_token TEXT NOT NULL,
    status             TEXT NOT NULL DEFAULT 'pending',  -- pending, verified, failed
    created_at         TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    verified_at        TEXT,
    PRIMARY KEY (tenant_id, domain)
);

-- Only one tenant can own a verified domain
CREATE UNIQUE INDEX idx_tenant_domains_verified
    ON tenant_domains(domain) WHERE status = 'verified';
```

The partial unique index on `domain WHERE status = 'verified'` ensures that only one tenant can own a given domain at a time, while allowing multiple tenants to have pending claims (race resolved on first verification).

### Signup Flow (Documented Pattern)

```rust
// In signup handler
async fn signup(
    body: JsonRequest<SignupForm>,
    Service(registry): Service<Arc<dyn DomainRegistry>>,
    Service(users): Service<UserService>,
) -> Result<Response> {
    let email = &body.email;

    // Check if email domain matches a verified tenant
    if let Some(tenant_match) = registry.lookup_email(email).await? {
        // Auto-assign to tenant
        let user = users.create(email, Some(&tenant_match.tenant_id)).await?;
        // ... create session, redirect
    } else {
        // Normal signup without tenant
        let user = users.create(email, None).await?;
        // ...
    }
}
```

### Domain Claim Flow (Documented Pattern)

```rust
// Admin adds a domain
async fn add_domain(
    tenant: Tenant<MyTenant>,
    Service(registry): Service<Arc<dyn DomainRegistry>>,
    body: JsonRequest<AddDomainForm>,
) -> Result<Json<DomainClaim>> {
    let claim = registry.register(tenant.id(), &body.domain).await?;
    // Response includes verification_token
    // Admin sets TXT record: _modo-verify.acme.com → {verification_token}
    Ok(Json(claim))
}

// Admin triggers verification check
async fn verify_domain(
    tenant: Tenant<MyTenant>,
    Service(registry): Service<Arc<dyn DomainRegistry>>,
    Path(domain): Path<String>,
) -> Result<Json<DomainClaim>> {
    let status = registry.verify(tenant.id(), &domain).await?;
    // Returns updated status
}
```

### Design Notes

- `DomainRegistry` is `Arc<dyn DomainRegistry>` — testable, mockable
- Email domain extraction: split on `@`, take the right side, lowercase
- `lookup_email()` only matches `status = 'verified'` domains
- A domain can only belong to one tenant at a time (enforced by partial unique index)
- Multiple tenants can have `pending` claims for the same domain — first to verify wins
- Verification token format: reuses `dns::generate_verification_token()` for consistency
- TXT record convention: `_modo-verify.{domain}` with the token as value (matches existing `dns` module pattern)
- The module does NOT handle email verification (proving the user owns the email address) — that's the app's responsibility. This module only handles domain ownership verification and email-to-tenant matching
- Works without `dns` feature: if constructed without a `DomainVerifier`, the app must call `verify()` with an external mechanism or manual approval

---

## Cross-Cutting Concerns

### Error Handling

All modules use `modo::Error` and `modo::Result<T>`. New error conditions:

- `filter`: `Error::bad_request()` for invalid filter syntax, unknown fields
- `page`: Silent clamping for out-of-range `per_page` — no errors
- `audit`: Logging errors are traced but optionally swallowed (configurable)
- `flag`: Unknown flags return `false` — no errors
- `tier`: `Error::forbidden()` for plan-gated features
- `export`: `Error::internal()` for serialization failures
- `domain_signup`: `Error::conflict()` if domain already verified by another tenant

### Testing

Each module provides:

- Unit tests for core logic (builders, parsers, evaluation)
- Integration tests using `TestApp` and `TestDb` where DB is involved
- Test helpers where useful (e.g., `flag::test::always_enabled()`, `tier::test::free_tier()`)

### Configuration

Modules that read from YAML follow modo's existing `Config` pattern with `${VAR}` substitution. New config sections nest under their module name (`pagination:`, `flags:`, `http:`, etc.).
