# Audit Log Module Design — 2026-03-30

Detailed design for the `audit` module: explicit event logging for business-significant actions. SQLite-backed with a universal schema supporting single-tenant and multi-tenant apps.

Refines section 1 of [new-modules-design.md](2026-03-27-new-modules-design.md).

## Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Error handling | Two methods: `record()` propagates, `record_silent()` traces and swallows | Explicit intent at call site, zero config |
| Schema mapping | Fixed `FromRow` for hardcoded column layout | Simple, predictable; custom schemas use `AuditLogBackend` trait |
| Metadata API | `impl Serialize` via builder method | Supports typed structs and `json!()`; `None` when omitted |
| Entry construction | Builder with 4 required positional args | `actor`, `action`, `resource_type`, `resource_id` enforced at compile time |
| Client context | `ClientInfo` extractor in `src/extractor/` | Shared struct for IP, user-agent, fingerprint; reusable by session module later |
| Repo filter fields | Generic `query()` with `FilterSchema` only | No dedicated `by_ip()` etc. — filter handles arbitrary field combinations |
| Table name | Hardcoded `audit_log` | Custom table names use `AuditLogBackend` trait directly |

## Module Structure

```
src/audit/
  mod.rs          — pub mod + re-exports
  entry.rs        — AuditEntry builder
  record.rs       — AuditRecord (stored form, FromRow)
  backend.rs      — AuditLogBackend trait
  log.rs          — AuditLog concrete wrapper
  repo.rs         — AuditRepo query interface

src/extractor/
  client_info.rs  — ClientInfo struct + FromRequestParts
```

No feature flag — always available (requires `db` for built-in backend). No new dependencies. No middleware.

---

## `ClientInfo` Extractor

New shared struct in `src/extractor/client_info.rs`:

```rust
#[derive(Debug, Clone, Default)]
pub struct ClientInfo {
    pub ip: Option<String>,
    pub user_agent: Option<String>,
    pub fingerprint: Option<String>,
}
```

### Extraction (FromRequestParts)

- `ip`: from existing `modo::ip` extraction logic (ConnectInfo / X-Forwarded-For / trusted proxies)
- `user_agent`: from `User-Agent` header
- `fingerprint`: from `X-Fingerprint` header (same convention as session module)

### Manual Construction

For non-HTTP contexts (background jobs, CLI tools, tests):

```rust
impl ClientInfo {
    pub fn new() -> Self { Self::default() }
    pub fn ip(mut self, ip: impl Into<String>) -> Self;
    pub fn user_agent(mut self, ua: impl Into<String>) -> Self;
    pub fn fingerprint(mut self, fp: impl Into<String>) -> Self;
}
```

Session module refactoring to use shared `ClientInfo` is out of scope for this work.

---

## `AuditEntry` Builder

What the caller provides. Four required positional fields, optional builder methods:

```rust
pub struct AuditEntry {
    actor: String,
    action: String,
    resource_type: String,
    resource_id: String,
    metadata: Option<serde_json::Value>,
    client_info: Option<ClientInfo>,
    tenant_id: Option<String>,
}

impl AuditEntry {
    pub fn new(
        actor: impl Into<String>,
        action: impl Into<String>,
        resource_type: impl Into<String>,
        resource_id: impl Into<String>,
    ) -> Self;

    /// Serialize any type into the metadata JSON field.
    pub fn metadata(mut self, meta: impl Serialize) -> Self {
        self.metadata = Some(serde_json::to_value(meta).unwrap_or_default());
        self
    }

    /// Attach client context (IP, user-agent, fingerprint).
    pub fn client_info(mut self, info: ClientInfo) -> Self;

    /// Set tenant ID for multi-tenant apps.
    pub fn tenant_id(mut self, id: impl Into<String>) -> Self;
}
```

Conventions:
- Actions use dot-notation: `resource.verb` or `resource.sub.verb` (e.g., `user.role.changed`, `account.deleted`)
- Actor is a string: user ID, `"system"`, API key ID — app decides the convention

---

## `AuditRecord`

Stored form returned by repo queries. Flat structure — `ClientInfo` fields are individual columns:

```rust
#[derive(Debug, Clone, Serialize)]
pub struct AuditRecord {
    pub id: String,                 // ULID
    pub actor: String,
    pub action: String,
    pub resource_type: String,
    pub resource_id: String,
    pub metadata: serde_json::Value,
    pub ip: Option<String>,
    pub user_agent: Option<String>,
    pub fingerprint: Option<String>,
    pub tenant_id: Option<String>,
    pub created_at: String,
}

impl FromRow for AuditRecord { ... }
```

---

## `AuditLogBackend` Trait

Object-safe backend trait with a single method:

```rust
pub trait AuditLogBackend: Send + Sync {
    fn record(
        &self,
        entry: &AuditEntry,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>>;
}
```

The built-in SQLite implementation:
1. Generates a ULID for the `id` column
2. Flattens `ClientInfo` into `ip`, `user_agent`, `fingerprint` columns
3. Serializes `metadata` to JSON string (or `'{}'` if `None`)
4. Inserts into the `audit_log` table

---

## `AuditLog` Concrete Wrapper

```rust
#[derive(Clone)]
pub struct AuditLog(Arc<dyn AuditLogBackend>);

impl AuditLog {
    /// Built-in SQLite backend. Table name: `audit_log`.
    pub fn new(db: Database) -> Self;

    /// Custom backend.
    pub fn from_backend(backend: Arc<dyn AuditLogBackend>) -> Self;

    /// Record an audit event. Propagates errors via Result.
    pub async fn record(&self, entry: &AuditEntry) -> Result<()>;

    /// Record an audit event. Traces errors, never fails.
    pub async fn record_silent(&self, entry: &AuditEntry);
}
```

`record_silent` traces errors with structured fields:

```rust
pub async fn record_silent(&self, entry: &AuditEntry) {
    if let Err(e) = self.0.record(entry).await {
        tracing::error!(
            error = %e,
            action = %entry.action,
            actor = %entry.actor,
            "audit log write failed"
        );
    }
}
```

Registered via `.with_service(audit_log)`, extracted as `Service(audit): Service<AuditLog>`.

---

## `AuditRepo` Query Interface

```rust
#[derive(Clone)]
pub struct AuditRepo {
    inner: Arc<AuditRepoInner>,
}

struct AuditRepoInner {
    db: Database,
}

impl AuditRepo {
    pub fn new(db: Database) -> Self;

    pub async fn list(&self, req: &PageRequest) -> Result<Page<AuditRecord>>;
    pub async fn by_actor(&self, actor: &str, req: &PageRequest) -> Result<Page<AuditRecord>>;
    pub async fn by_resource(&self, resource_type: &str, resource_id: &str, req: &PageRequest) -> Result<Page<AuditRecord>>;
    pub async fn by_tenant(&self, tenant_id: &str, req: &PageRequest) -> Result<Page<AuditRecord>>;
    pub async fn by_action(&self, action: &str, req: &PageRequest) -> Result<Page<AuditRecord>>;
    pub async fn query(&self, filter: &ValidatedFilter, req: &PageRequest) -> Result<Page<AuditRecord>>;
}
```

All dedicated methods use `SelectBuilder` internally — convenience wrappers that add a WHERE clause and delegate to `.page::<AuditRecord>()`.

### Filter Schema (for `query()`)

Filterable fields: `actor` (Text), `action` (Text), `resource_type` (Text), `resource_id` (Text), `ip` (Text), `user_agent` (Text), `fingerprint` (Text), `tenant_id` (Text), `created_at` (Date).

Sortable: `created_at`, `action`.

---

## Recommended Schema

Documented in module docs. Migration owned by end app.

```sql
CREATE TABLE audit_log (
    id              TEXT PRIMARY KEY,
    actor           TEXT NOT NULL,
    action          TEXT NOT NULL,
    resource_type   TEXT NOT NULL,
    resource_id     TEXT NOT NULL,
    metadata        TEXT NOT NULL DEFAULT '{}',
    ip              TEXT,
    user_agent      TEXT,
    fingerprint     TEXT,
    tenant_id       TEXT,
    created_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE INDEX idx_audit_actor      ON audit_log(actor);
CREATE INDEX idx_audit_resource   ON audit_log(resource_type, resource_id);
CREATE INDEX idx_audit_action     ON audit_log(action);
CREATE INDEX idx_audit_tenant     ON audit_log(tenant_id) WHERE tenant_id IS NOT NULL;
CREATE INDEX idx_audit_created    ON audit_log(created_at);
```

---

## Usage Examples

### Recording in a handler

```rust
async fn change_role(
    session: Session,
    client: ClientInfo,
    Service(audit): Service<AuditLog>,
    Service(users): Service<UserService>,
    Path(user_id): Path<String>,
    body: JsonRequest<ChangeRoleForm>,
) -> Result<()> {
    let old_role = users.get_role(&user_id).await?;
    users.set_role(&user_id, &body.role).await?;

    audit.record(
        &AuditEntry::new(&session.user_id(), "user.role.changed", "user", &user_id)
            .metadata(serde_json::json!({ "old_role": old_role, "new_role": body.role }))
            .client_info(client)
    ).await?;

    Ok(())
}
```

### Background job (no request context)

```rust
async fn cleanup_expired_accounts(audit: &AuditLog, deleted_ids: &[String]) {
    for id in deleted_ids {
        audit.record_silent(
            &AuditEntry::new("system", "account.deleted", "user", id)
        ).await;
    }
}
```

### Querying audit history

```rust
async fn user_audit_trail(
    Path(user_id): Path<String>,
    page: PageRequest,
    Service(repo): Service<AuditRepo>,
) -> Result<Json<Page<AuditRecord>>> {
    Ok(Json(repo.by_actor(&user_id, &page).await?))
}
```

### Querying with filters

```rust
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
        .field("ip", FieldType::Text)
        .field("user_agent", FieldType::Text)
        .field("fingerprint", FieldType::Text)
        .field("tenant_id", FieldType::Text)
        .field("created_at", FieldType::Date)
        .sort_fields(&["created_at", "action"]);

    let validated = filter.validate(&schema)?;
    Ok(Json(repo.query(&validated, &page).await?))
}
```

### Wiring

```rust
let audit_log = AuditLog::new(db.clone());
let audit_repo = AuditRepo::new(db.clone());

let app = Router::new()
    .route("/api/users/{id}/role", put(change_role))
    .route("/api/users/{id}/audit", get(user_audit_trail))
    .route("/api/audit", get(search_audit))
    .with_service(audit_log)
    .with_service(audit_repo);
```

---

## Testing

### Unit tests

- `AuditEntry` builder: verify required fields, metadata serialization, `ClientInfo` attachment
- `AuditRecord` `FromRow`: round-trip through SQLite row mapping

### Integration tests (TestDb)

- Built-in backend: insert via `record()`, read back via repo methods
- `record_silent()`: verify no panic on DB failure, error is traced
- Repo pagination: multiple entries, verify `Page` metadata
- Repo filter: verify `query()` with various filter combinations

### Test helpers

In-memory backend for app-level tests:

```rust
struct MemoryAuditBackend {
    entries: Arc<std::sync::Mutex<Vec<AuditEntry>>>,
}

impl AuditLogBackend for MemoryAuditBackend {
    fn record(&self, entry: &AuditEntry) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
        Box::pin(async move {
            self.entries.lock().unwrap().push(entry.clone());
            Ok(())
        })
    }
}
```

Exposed via `AuditLog::memory()` constructor (gated behind `#[cfg(any(test, feature = "audit-test"))]`).

---

## Error Handling

- `record()`: propagates `modo::Error` — handler decides via `?` or explicit handling
- `record_silent()`: traces error with `tracing::error!`, returns nothing
- `AuditRepo` methods: return `modo::Result<Page<AuditRecord>>` — standard error propagation
- No custom error codes — audit failures are infrastructure errors (`Error::internal()`)

## Design Notes

- `AuditLog` wraps `Arc<dyn AuditLogBackend>` — no `Arc<Inner>` pattern needed (the trait object is the inner)
- `AuditRepo` uses `Arc<Inner>` pattern (holds `Database` which is already `Arc`-backed, but follows framework convention)
- Handlers explicitly log what matters — no automatic middleware capture
- `metadata` is unstructured JSON — keeps the schema stable while allowing arbitrary context
- `tenant_id` is `NULL` for single-tenant apps; the partial index has zero overhead when all values are `NULL`
- `ClientInfo` fields stored as individual columns (not nested JSON) for direct filtering
- `AuditEntry` must derive `Clone` so the in-memory test backend can store copies
