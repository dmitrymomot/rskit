# Audit

Explicit audit logging for business-significant actions. Feature-gated under `db` (default).

```toml
# Cargo.toml — db is a default feature, no extra config needed
modo = { path = ".." }
```

All types are re-exported from the crate root under `#[cfg(feature = "db")]`:

```rust
use modo::{
    AuditEntry, AuditLog, AuditLogBackend, AuditRecord, AuditRepo,
};
```

`ClientInfo` is always available (no feature gate):

```rust
use modo::ClientInfo;
// or
use modo::extractor::ClientInfo;
```

`MemoryAuditBackend` is only available under `#[cfg(test)]` or `feature = "test-helpers"`:

```rust
use modo::audit::MemoryAuditBackend;
```

Source: `src/audit/` (mod.rs, entry.rs, record.rs, backend.rs, log.rs, repo.rs), `src/extractor/client_info.rs`.

---

## ClientInfo

Client request context extracted from HTTP requests. Private fields with builder for non-HTTP contexts.

```rust
#[derive(Debug, Clone, Default)]
pub struct ClientInfo { /* private fields: ip, user_agent, fingerprint */ }
```

### Construction

```rust
// Builder (for non-HTTP contexts like background jobs):
let info = ClientInfo::new()
    .ip("1.2.3.4")
    .user_agent("my-script/1.0")
    .fingerprint("abc123");

// Automatic extraction in handlers (implements FromRequestParts):
async fn handler(info: ClientInfo) { /* ... */ }
```

### Constructors and methods

```rust
pub fn new() -> Self
pub fn ip(self, ip: impl Into<String>) -> Self
pub fn user_agent(self, ua: impl Into<String>) -> Self
pub fn fingerprint(self, fp: impl Into<String>) -> Self
pub fn ip_value(&self) -> Option<&str>
pub fn user_agent_value(&self) -> Option<&str>
pub fn fingerprint_value(&self) -> Option<&str>
```

- `new()` returns all fields as `None`.
- Builder methods consume and return `Self`.
- `FromRequestParts` impl reads `ClientIp` from extensions (requires `ClientIpLayer`), `User-Agent` header, and `x-fingerprint` header. Missing values become `None` — extraction never fails.

---

## AuditEntry

Builder for audit events. Four required fields, three optional.

```rust
#[derive(Debug, Clone)]
pub struct AuditEntry { /* private fields */ }
```

### Construction

```rust
let entry = AuditEntry::new("user_123", "user.role.changed", "user", "usr_abc")
    .metadata(serde_json::json!({"old_role": "editor", "new_role": "admin"}))
    .client_info(info)
    .tenant_id("tenant_1");
```

### Constructors and methods

```rust
pub fn new(
    actor: impl Into<String>,
    action: impl Into<String>,
    resource_type: impl Into<String>,
    resource_id: impl Into<String>,
) -> Self
pub fn metadata(self, meta: serde_json::Value) -> Self
pub fn client_info(self, info: ClientInfo) -> Self
pub fn tenant_id(self, id: impl Into<String>) -> Self
pub fn actor(&self) -> &str
pub fn action(&self) -> &str
pub fn resource_type(&self) -> &str
pub fn resource_id(&self) -> &str
pub fn metadata_value(&self) -> Option<&serde_json::Value>
pub fn client_info_value(&self) -> Option<&ClientInfo>
pub fn tenant_id_value(&self) -> Option<&str>
```

- `metadata()` accepts `serde_json::Value` directly. Use `serde_json::json!()` for inline values or `serde_json::to_value(my_struct).unwrap()` for custom types.
- All builder methods consume and return `Self`.
- Accessor methods return references to the inner data.

---

## AuditRecord

Stored audit event returned by queries. All fields flat — `ClientInfo` is expanded into separate columns.

```rust
#[derive(Debug, Clone, Serialize)]
pub struct AuditRecord {
    pub id: String,
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
```

Implements `FromRow` for automatic mapping from SQLite rows. The `metadata` column is stored as a JSON text string and parsed on read — returns an error if the stored JSON is corrupted.

---

## AuditLogBackend

Object-safe trait for custom storage backends.

```rust
pub trait AuditLogBackend: Send + Sync {
    fn record<'a>(
        &'a self,
        entry: &'a AuditEntry,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>;
}
```

Must use `Pin<Box<dyn Future>>` (not RPITIT) for object safety behind `Arc<dyn AuditLogBackend>`.

---

## AuditLog

Concrete audit log service. Wraps `Arc<dyn AuditLogBackend>` for cheap cloning.

```rust
#[derive(Clone)]
pub struct AuditLog(/* Arc<dyn AuditLogBackend> */);
```

### Construction

```rust
// Built-in SQLite backend:
let audit = AuditLog::new(db);

// Custom backend:
let audit = AuditLog::from_backend(my_backend);

// In-memory for testing (requires #[cfg(test)] or feature = "test-helpers"):
let (audit, backend) = AuditLog::memory();
```

### Constructors and methods

```rust
pub fn new(db: Database) -> Self
pub fn from_backend(backend: Arc<dyn AuditLogBackend>) -> Self
pub async fn record(&self, entry: &AuditEntry) -> Result<()>
pub async fn record_silent(&self, entry: &AuditEntry)
```

Test-only:

```rust
#[cfg(any(test, feature = "test-helpers"))]
pub fn memory() -> (Self, Arc<MemoryAuditBackend>)
```

- `record()` propagates backend errors via `Result`.
- `record_silent()` traces errors with `tracing::error!` (action + actor fields) and swallows them — never fails.
- `new()` uses the built-in `SqliteAuditBackend` which INSERTs into the `audit_log` table with a ULID `id`.
- `memory()` returns both the `AuditLog` and a handle to inspect captured entries.

---

## MemoryAuditBackend

In-memory backend for testing. Only available under `#[cfg(test)]` or `feature = "test-helpers"`.

```rust
pub struct MemoryAuditBackend { /* Mutex<Vec<AuditEntry>> */ }
```

### Methods

```rust
pub fn entries(&self) -> Vec<AuditEntry>
```

- `entries()` clones and returns all captured entries.
- Created via `AuditLog::memory()`, not directly.

---

## AuditRepo

Query interface for audit records. Uses cursor pagination (keyset on `id` column, newest first).

```rust
#[derive(Clone)]
pub struct AuditRepo { /* Arc<Inner> pattern */ }
```

### Construction

```rust
let repo = AuditRepo::new(db);
```

### Constructors and methods

```rust
pub fn new(db: Database) -> Self
pub async fn list(&self, req: CursorRequest) -> Result<CursorPage<AuditRecord>>
pub async fn query(&self, filter: ValidatedFilter, req: CursorRequest) -> Result<CursorPage<AuditRecord>>
```

- `list()` returns all entries, newest first, with cursor pagination.
- `query()` applies a `ValidatedFilter` for flexible WHERE conditions (actor, action, resource, tenant, date ranges, etc.).
- Both use `SelectBuilder::cursor()` which paginates on the `id` column by default.
- `CursorRequest` has fields `after: Option<String>` and `per_page: i64`.
- `CursorPage<T>` has fields `items: Vec<T>`, `next_cursor: Option<String>`, `has_more: bool`, `per_page: i64`.

---

## Schema

The audit module does NOT ship migrations. End-apps own the schema. Required table:

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
```

---

## Gotchas

- `metadata()` accepts `serde_json::Value`, not `impl Serialize`. Serialize at the callsite with `serde_json::json!()` or `serde_json::to_value()`.
- `ClientInfo` fields are private. Use `ip_value()`, `user_agent_value()`, `fingerprint_value()` accessors.
- `CursorRequest` does not implement `Default`. Construct explicitly: `CursorRequest { after: None, per_page: 20 }`.
- `AuditLogBackend` uses `Pin<Box<dyn Future>>` with explicit lifetime `'a` — required for object safety.
- `record_silent()` logs errors but never returns them — use `record()` when you need to handle write failures.
- The `audit_log` table must exist before using `AuditLog` or `AuditRepo` — the module doesn't create it.
