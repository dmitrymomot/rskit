# Audit Module Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an `audit` module to the modo framework for explicit event logging with SQLite-backed storage, builder-based entries, and paginated querying.

**Architecture:** `AuditLog` concrete wrapper over `Arc<dyn AuditLogBackend>` for writes, `AuditRepo` with `Arc<Inner>` pattern for reads. A shared `ClientInfo` extractor captures IP/user-agent/fingerprint from request parts. No middleware — handlers call `record()` or `record_silent()` explicitly.

**Tech Stack:** Rust 2024, libsql, serde_json, axum extractors, Tower (no middleware needed), existing modo db/filter/page infrastructure.

**Spec:** `docs/superpowers/specs/2026-03-30-audit-module-design.md`

---

### Task 1: `ClientInfo` Extractor

**Files:**
- Create: `src/extractor/client_info.rs`
- Modify: `src/extractor/mod.rs`

- [ ] **Step 1: Write unit tests for `ClientInfo` builder**

In `src/extractor/client_info.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_has_all_none() {
        let info = ClientInfo::new();
        assert!(info.ip.is_none());
        assert!(info.user_agent.is_none());
        assert!(info.fingerprint.is_none());
    }

    #[test]
    fn builder_sets_fields() {
        let info = ClientInfo::new()
            .ip("1.2.3.4")
            .user_agent("Mozilla/5.0")
            .fingerprint("abc123");
        assert_eq!(info.ip.as_deref(), Some("1.2.3.4"));
        assert_eq!(info.user_agent.as_deref(), Some("Mozilla/5.0"));
        assert_eq!(info.fingerprint.as_deref(), Some("abc123"));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib extractor::client_info::tests -- --nocapture`
Expected: compilation error — module and struct don't exist yet.

- [ ] **Step 3: Implement `ClientInfo` struct and builder**

In `src/extractor/client_info.rs`:

```rust
use axum::extract::FromRequestParts;
use http::request::Parts;

use crate::error::Error;
use crate::ip::ClientIp;

/// Client request context: IP address, user-agent, and fingerprint.
///
/// Implements [`FromRequestParts`] for automatic extraction in handlers.
/// Requires [`ClientIpLayer`](crate::ClientIpLayer) for the `ip` field;
/// if the layer is absent, `ip` will be `None`.
///
/// For non-HTTP contexts (background jobs, CLI tools), use the builder:
///
/// ```
/// use modo::extractor::ClientInfo;
///
/// let info = ClientInfo::new()
///     .ip("1.2.3.4")
///     .user_agent("my-script/1.0");
/// ```
#[derive(Debug, Clone, Default)]
pub struct ClientInfo {
    pub ip: Option<String>,
    pub user_agent: Option<String>,
    pub fingerprint: Option<String>,
}

impl ClientInfo {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn ip(mut self, ip: impl Into<String>) -> Self {
        self.ip = Some(ip.into());
        self
    }

    pub fn user_agent(mut self, ua: impl Into<String>) -> Self {
        self.user_agent = Some(ua.into());
        self
    }

    pub fn fingerprint(mut self, fp: impl Into<String>) -> Self {
        self.fingerprint = Some(fp.into());
        self
    }
}

impl<S: Send + Sync> FromRequestParts<S> for ClientInfo {
    type Rejection = Error;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let ip = parts.extensions.get::<ClientIp>().map(|c| c.0.to_string());

        let user_agent = parts
            .headers
            .get(http::header::USER_AGENT)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        let fingerprint = parts
            .headers
            .get("x-fingerprint")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        Ok(Self {
            ip,
            user_agent,
            fingerprint,
        })
    }
}
```

- [ ] **Step 4: Register the module in `src/extractor/mod.rs`**

Add after the existing `mod service;` line:

```rust
mod client_info;
```

Add to re-exports:

```rust
pub use client_info::ClientInfo;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --lib extractor::client_info::tests -- --nocapture`
Expected: 2 tests pass.

- [ ] **Step 6: Write extractor test**

Add to the `tests` module in `src/extractor/client_info.rs`:

```rust
    #[tokio::test]
    async fn extracts_from_request_parts() {
        use crate::ip::ClientIp;
        use std::net::IpAddr;

        let mut req = http::Request::builder()
            .header("user-agent", "TestAgent/1.0")
            .header("x-fingerprint", "fp_abc")
            .body(())
            .unwrap();
        let ip: IpAddr = "10.0.0.1".parse().unwrap();
        req.extensions_mut().insert(ClientIp(ip));

        let (mut parts, _) = req.into_parts();
        let info = ClientInfo::from_request_parts(&mut parts, &()).await.unwrap();

        assert_eq!(info.ip.as_deref(), Some("10.0.0.1"));
        assert_eq!(info.user_agent.as_deref(), Some("TestAgent/1.0"));
        assert_eq!(info.fingerprint.as_deref(), Some("fp_abc"));
    }

    #[tokio::test]
    async fn extracts_with_missing_fields() {
        let req = http::Request::builder().body(()).unwrap();
        let (mut parts, _) = req.into_parts();
        let info = ClientInfo::from_request_parts(&mut parts, &()).await.unwrap();

        assert!(info.ip.is_none());
        assert!(info.user_agent.is_none());
        assert!(info.fingerprint.is_none());
    }
```

- [ ] **Step 7: Run all extractor tests**

Run: `cargo test --lib extractor::client_info::tests -- --nocapture`
Expected: 4 tests pass.

- [ ] **Step 8: Commit**

```bash
git add src/extractor/client_info.rs src/extractor/mod.rs
git commit -m "feat(extractor): add ClientInfo extractor for IP, user-agent, fingerprint"
```

---

### Task 2: `AuditEntry` Builder

**Files:**
- Create: `src/audit/entry.rs`
- Create: `src/audit/mod.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Write unit tests for `AuditEntry`**

In `src/audit/entry.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_sets_required_fields() {
        let entry = AuditEntry::new("user_123", "user.created", "user", "usr_abc");
        assert_eq!(entry.actor(), "user_123");
        assert_eq!(entry.action(), "user.created");
        assert_eq!(entry.resource_type(), "user");
        assert_eq!(entry.resource_id(), "usr_abc");
        assert!(entry.metadata_value().is_none());
        assert!(entry.client_info_value().is_none());
        assert!(entry.tenant_id_value().is_none());
    }

    #[test]
    fn metadata_with_json_value() {
        let entry = AuditEntry::new("user_123", "user.role.changed", "user", "usr_abc")
            .metadata(serde_json::json!({"old_role": "editor", "new_role": "admin"}));
        let meta = entry.metadata_value().unwrap();
        assert_eq!(meta["old_role"], "editor");
        assert_eq!(meta["new_role"], "admin");
    }

    #[test]
    fn metadata_with_serializable_struct() {
        #[derive(serde::Serialize)]
        struct RoleChange {
            old_role: String,
            new_role: String,
        }

        let entry = AuditEntry::new("user_123", "user.role.changed", "user", "usr_abc")
            .metadata(RoleChange {
                old_role: "editor".into(),
                new_role: "admin".into(),
            });
        let meta = entry.metadata_value().unwrap();
        assert_eq!(meta["old_role"], "editor");
        assert_eq!(meta["new_role"], "admin");
    }

    #[test]
    fn client_info_attached() {
        use crate::extractor::ClientInfo;

        let info = ClientInfo::new().ip("1.2.3.4").user_agent("Bot/1.0");
        let entry = AuditEntry::new("system", "job.ran", "job", "job_1")
            .client_info(info);
        let ci = entry.client_info_value().unwrap();
        assert_eq!(ci.ip.as_deref(), Some("1.2.3.4"));
        assert_eq!(ci.user_agent.as_deref(), Some("Bot/1.0"));
    }

    #[test]
    fn tenant_id_set() {
        let entry = AuditEntry::new("user_123", "doc.deleted", "document", "doc_1")
            .tenant_id("tenant_abc");
        assert_eq!(entry.tenant_id_value(), Some("tenant_abc"));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib audit::entry::tests -- --nocapture`
Expected: compilation error — module doesn't exist yet.

- [ ] **Step 3: Implement `AuditEntry`**

In `src/audit/entry.rs`:

```rust
use serde::Serialize;

use crate::extractor::ClientInfo;

/// An audit event to be recorded.
///
/// Constructed with four required fields (actor, action, resource_type,
/// resource_id) and optional builder methods for metadata, client context,
/// and tenant.
///
/// ```
/// use modo::audit::AuditEntry;
///
/// let entry = AuditEntry::new("user_123", "user.role.changed", "user", "usr_abc")
///     .metadata(serde_json::json!({"old_role": "editor"}))
///     .tenant_id("tenant_1");
/// ```
#[derive(Debug, Clone)]
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
    ) -> Self {
        Self {
            actor: actor.into(),
            action: action.into(),
            resource_type: resource_type.into(),
            resource_id: resource_id.into(),
            metadata: None,
            client_info: None,
            tenant_id: None,
        }
    }

    /// Serialize any type into the metadata JSON field.
    pub fn metadata(mut self, meta: impl Serialize) -> Self {
        self.metadata = Some(serde_json::to_value(meta).unwrap_or_default());
        self
    }

    /// Attach client context (IP, user-agent, fingerprint).
    pub fn client_info(mut self, info: ClientInfo) -> Self {
        self.client_info = Some(info);
        self
    }

    /// Set tenant ID for multi-tenant apps.
    pub fn tenant_id(mut self, id: impl Into<String>) -> Self {
        self.tenant_id = Some(id.into());
        self
    }

    pub fn actor(&self) -> &str {
        &self.actor
    }

    pub fn action(&self) -> &str {
        &self.action
    }

    pub fn resource_type(&self) -> &str {
        &self.resource_type
    }

    pub fn resource_id(&self) -> &str {
        &self.resource_id
    }

    pub fn metadata_value(&self) -> Option<&serde_json::Value> {
        self.metadata.as_ref()
    }

    pub fn client_info_value(&self) -> Option<&ClientInfo> {
        self.client_info.as_ref()
    }

    pub fn tenant_id_value(&self) -> Option<&str> {
        self.tenant_id.as_deref()
    }
}
```

- [ ] **Step 4: Create `src/audit/mod.rs`**

```rust
//! Audit logging for business-significant actions.
//!
//! Provides explicit event recording (no automatic middleware capture) with
//! a pluggable backend trait and a built-in SQLite implementation.
//!
//! | Type | Purpose |
//! |---|---|
//! | [`AuditEntry`] | Builder for audit events — four required fields plus optional metadata, client info, tenant |
//! | [`AuditRecord`] | Stored form returned by queries — all fields flat, includes `id` and `created_at` |
//! | [`AuditLogBackend`] | Object-safe trait for custom storage backends |
//! | [`AuditLog`] | Concrete wrapper — `record()` propagates errors, `record_silent()` traces and swallows |
//! | [`AuditRepo`] | Query interface — dedicated methods plus generic filter-based `query()` |

mod entry;

pub use entry::AuditEntry;
```

- [ ] **Step 5: Register audit module in `src/lib.rs`**

Add after the `#[cfg(feature = "db")] pub mod db;` line:

```rust
#[cfg(feature = "db")]
pub mod audit;
```

- [ ] **Step 6: Run tests**

Run: `cargo test --lib audit::entry::tests -- --nocapture`
Expected: 5 tests pass.

- [ ] **Step 7: Commit**

```bash
git add src/audit/entry.rs src/audit/mod.rs src/lib.rs
git commit -m "feat(audit): add AuditEntry builder with metadata and client info"
```

---

### Task 3: `AuditRecord` with `FromRow`

**Files:**
- Create: `src/audit/record.rs`
- Modify: `src/audit/mod.rs`

- [ ] **Step 1: Write `AuditRecord` struct and `FromRow` impl**

In `src/audit/record.rs`:

```rust
use serde::Serialize;

use crate::db::from_row::{ColumnMap, FromRow};
use crate::error::Result;

/// Stored audit event returned by [`AuditRepo`](super::AuditRepo) queries.
///
/// All fields are flat — [`ClientInfo`](crate::extractor::ClientInfo) is
/// expanded into `ip`, `user_agent`, `fingerprint` columns.
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

impl FromRow for AuditRecord {
    fn from_row(row: &libsql::Row) -> Result<Self> {
        let cols = ColumnMap::from_row(row);
        let metadata_str: String = cols.get(row, "metadata")?;
        let metadata: serde_json::Value =
            serde_json::from_str(&metadata_str).unwrap_or_default();

        Ok(Self {
            id: cols.get(row, "id")?,
            actor: cols.get(row, "actor")?,
            action: cols.get(row, "action")?,
            resource_type: cols.get(row, "resource_type")?,
            resource_id: cols.get(row, "resource_id")?,
            metadata,
            ip: cols.get(row, "ip")?,
            user_agent: cols.get(row, "user_agent")?,
            fingerprint: cols.get(row, "fingerprint")?,
            tenant_id: cols.get(row, "tenant_id")?,
            created_at: cols.get(row, "created_at")?,
        })
    }
}
```

- [ ] **Step 2: Register in `src/audit/mod.rs`**

Add:

```rust
mod record;

pub use record::AuditRecord;
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check --features db`
Expected: success.

- [ ] **Step 4: Commit**

```bash
git add src/audit/record.rs src/audit/mod.rs
git commit -m "feat(audit): add AuditRecord with FromRow for SQLite mapping"
```

---

### Task 4: `AuditLogBackend` Trait and `AuditLog` Wrapper

**Files:**
- Create: `src/audit/backend.rs`
- Create: `src/audit/log.rs`
- Modify: `src/audit/mod.rs`

- [ ] **Step 1: Define `AuditLogBackend` trait**

In `src/audit/backend.rs`:

```rust
use std::pin::Pin;

use crate::error::Result;

use super::entry::AuditEntry;

/// Object-safe backend trait for audit log storage.
///
/// Implement this trait to use a custom storage backend (e.g., remote
/// logging service, file-based). The built-in SQLite backend is used
/// by [`AuditLog::new()`](super::AuditLog::new).
pub trait AuditLogBackend: Send + Sync {
    /// Persist an audit entry.
    fn record(
        &self,
        entry: &AuditEntry,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>>;
}
```

- [ ] **Step 2: Implement `AuditLog` wrapper with built-in SQLite backend**

In `src/audit/log.rs`:

```rust
use std::sync::Arc;

use crate::db::{ConnExt, Database};
use crate::error::Result;
use crate::id;

use super::backend::AuditLogBackend;
use super::entry::AuditEntry;

/// Concrete audit log service.
///
/// Wraps an [`AuditLogBackend`] behind `Arc` for cheap cloning.
/// Register with `.with_service(audit_log)` and extract as
/// `Service(audit): Service<AuditLog>`.
///
/// Two write methods:
/// - [`record()`](Self::record) — propagates errors via `Result`
/// - [`record_silent()`](Self::record_silent) — traces errors, never fails
#[derive(Clone)]
pub struct AuditLog(Arc<dyn AuditLogBackend>);

impl AuditLog {
    /// Create with the built-in SQLite backend writing to the `audit_log` table.
    pub fn new(db: Database) -> Self {
        Self(Arc::new(SqliteAuditBackend { db }))
    }

    /// Create with a custom backend.
    pub fn from_backend(backend: Arc<dyn AuditLogBackend>) -> Self {
        Self(backend)
    }

    /// Record an audit event. Propagates errors via `Result`.
    pub async fn record(&self, entry: &AuditEntry) -> Result<()> {
        self.0.record(entry).await
    }

    /// Record an audit event. Traces errors, never fails.
    pub async fn record_silent(&self, entry: &AuditEntry) {
        if let Err(e) = self.0.record(entry).await {
            tracing::error!(
                error = %e,
                action = %entry.action(),
                actor = %entry.actor(),
                "audit log write failed"
            );
        }
    }

    /// Create an in-memory audit log for testing.
    ///
    /// Returns the `AuditLog` and a handle to the backend for inspecting
    /// captured entries.
    #[cfg(any(test, feature = "audit-test"))]
    pub fn memory() -> (Self, Arc<MemoryAuditBackend>) {
        let backend = Arc::new(MemoryAuditBackend {
            entries: std::sync::Mutex::new(Vec::new()),
        });
        (Self(backend.clone()), backend)
    }
}

struct SqliteAuditBackend {
    db: Database,
}

impl AuditLogBackend for SqliteAuditBackend {
    fn record(
        &self,
        entry: &AuditEntry,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
        Box::pin(async move {
            let id = id::ulid();
            let metadata_json = entry
                .metadata_value()
                .map(|v| v.to_string())
                .unwrap_or_else(|| "{}".to_string());

            let (ip, user_agent, fingerprint) = match entry.client_info_value() {
                Some(ci) => (ci.ip.clone(), ci.user_agent.clone(), ci.fingerprint.clone()),
                None => (None, None, None),
            };

            self.db
                .conn()
                .execute_raw(
                    "INSERT INTO audit_log \
                     (id, actor, action, resource_type, resource_id, metadata, ip, user_agent, fingerprint, tenant_id) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                    libsql::params![
                        id,
                        entry.actor(),
                        entry.action(),
                        entry.resource_type(),
                        entry.resource_id(),
                        metadata_json,
                        ip,
                        user_agent,
                        fingerprint,
                        entry.tenant_id_value(),
                    ],
                )
                .await
                .map_err(crate::error::Error::from)?;

            Ok(())
        })
    }
}

/// In-memory audit backend for testing.
#[cfg(any(test, feature = "audit-test"))]
pub struct MemoryAuditBackend {
    entries: std::sync::Mutex<Vec<AuditEntry>>,
}

#[cfg(any(test, feature = "audit-test"))]
impl MemoryAuditBackend {
    /// Return a clone of all captured entries.
    pub fn entries(&self) -> Vec<AuditEntry> {
        self.entries.lock().unwrap().clone()
    }
}

#[cfg(any(test, feature = "audit-test"))]
impl AuditLogBackend for MemoryAuditBackend {
    fn record(
        &self,
        entry: &AuditEntry,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
        self.entries.lock().unwrap().push(entry.clone());
        Box::pin(async { Ok(()) })
    }
}
```

- [ ] **Step 3: Register in `src/audit/mod.rs`**

Add:

```rust
mod backend;
mod log;

pub use backend::AuditLogBackend;
pub use self::log::AuditLog;
#[cfg(any(test, feature = "audit-test"))]
pub use self::log::MemoryAuditBackend;
```

- [ ] **Step 4: Verify compilation**

Run: `cargo check --features db`
Expected: success.

- [ ] **Step 5: Commit**

```bash
git add src/audit/backend.rs src/audit/log.rs src/audit/mod.rs
git commit -m "feat(audit): add AuditLogBackend trait and AuditLog wrapper with SQLite backend"
```

---

### Task 5: `AuditRepo` Query Interface

**Files:**
- Create: `src/audit/repo.rs`
- Modify: `src/audit/mod.rs`

- [ ] **Step 1: Implement `AuditRepo`**

In `src/audit/repo.rs`.

The dedicated methods (`by_actor`, `by_resource`, etc.) use raw queries with `ConnQueryExt` because `SelectBuilder` only handles filter params — not params embedded in the base SQL. The `query()` method uses `SelectBuilder` with `ValidatedFilter` which manages its own params.

```rust
use std::sync::Arc;

use crate::db::filter::ValidatedFilter;
use crate::db::page::{Page, PageRequest};
use crate::db::{ConnExt, ConnQueryExt, Database};
use crate::error::{Error, Result};

use super::record::AuditRecord;

const COLS: &str = "id, actor, action, resource_type, resource_id, metadata, \
                    ip, user_agent, fingerprint, tenant_id, created_at";

/// Query interface for audit log records.
///
/// Provides dedicated methods for common access patterns and a generic
/// [`query()`](Self::query) method for flexible filtering via
/// [`ValidatedFilter`].
#[derive(Clone)]
pub struct AuditRepo {
    inner: Arc<AuditRepoInner>,
}

struct AuditRepoInner {
    db: Database,
}

impl AuditRepo {
    /// Create a new audit repo backed by the `audit_log` table.
    pub fn new(db: Database) -> Self {
        Self {
            inner: Arc::new(AuditRepoInner { db }),
        }
    }

    /// All entries, paginated, newest first.
    pub async fn list(&self, req: &PageRequest) -> Result<Page<AuditRecord>> {
        self.inner
            .db
            .conn()
            .select(&format!("SELECT {COLS} FROM audit_log"))
            .order_by("\"created_at\" DESC")
            .page::<AuditRecord>(req.clone())
            .await
    }

    /// Entries by actor (exact match), newest first.
    pub async fn by_actor(&self, actor: &str, req: &PageRequest) -> Result<Page<AuditRecord>> {
        self.paginated_where(
            "WHERE actor = ?1",
            libsql::params![actor],
            req,
        )
        .await
    }

    /// Entries by resource type and ID, newest first.
    pub async fn by_resource(
        &self,
        resource_type: &str,
        resource_id: &str,
        req: &PageRequest,
    ) -> Result<Page<AuditRecord>> {
        self.paginated_where(
            "WHERE resource_type = ?1 AND resource_id = ?2",
            libsql::params![resource_type, resource_id],
            req,
        )
        .await
    }

    /// Entries by tenant, newest first.
    pub async fn by_tenant(
        &self,
        tenant_id: &str,
        req: &PageRequest,
    ) -> Result<Page<AuditRecord>> {
        self.paginated_where(
            "WHERE tenant_id = ?1",
            libsql::params![tenant_id],
            req,
        )
        .await
    }

    /// Entries by action (exact match), newest first.
    pub async fn by_action(
        &self,
        action: &str,
        req: &PageRequest,
    ) -> Result<Page<AuditRecord>> {
        self.paginated_where(
            "WHERE action = ?1",
            libsql::params![action],
            req,
        )
        .await
    }

    /// Flexible query with a pre-validated filter.
    pub async fn query(
        &self,
        filter: &ValidatedFilter,
        req: &PageRequest,
    ) -> Result<Page<AuditRecord>> {
        self.inner
            .db
            .conn()
            .select(&format!("SELECT {COLS} FROM audit_log"))
            .filter(filter.clone())
            .order_by("\"created_at\" DESC")
            .page::<AuditRecord>(req.clone())
            .await
    }

    /// Internal helper: count + fetch with a WHERE clause and explicit params.
    async fn paginated_where(
        &self,
        where_clause: &str,
        params: impl libsql::params::IntoParams + Clone + Send,
        req: &PageRequest,
    ) -> Result<Page<AuditRecord>> {
        let count: i64 = self
            .inner
            .db
            .conn()
            .query_one_map(
                &format!("SELECT COUNT(*) FROM audit_log {where_clause}"),
                params.clone(),
                |row| Ok(row.get::<i64>(0).map_err(Error::from)?),
            )
            .await?;

        let items: Vec<AuditRecord> = self
            .inner
            .db
            .conn()
            .query_all(
                &format!(
                    "SELECT {COLS} FROM audit_log {where_clause} \
                     ORDER BY created_at DESC LIMIT {} OFFSET {}",
                    req.per_page,
                    req.offset()
                ),
                params,
            )
            .await?;

        Ok(Page::new(items, count, req.page, req.per_page))
    }
}
```

**Note on `paginated_where`:** We inline LIMIT/OFFSET as formatted values (not bind params) because the base SQL already uses `?1`, `?2` for the WHERE clause, and libsql uses positional params. This avoids param index conflicts. LIMIT/OFFSET come from `PageRequest` (not user input), so this is safe.

- [ ] **Step 2: Register in `src/audit/mod.rs`**

Add:

```rust
mod repo;

pub use repo::AuditRepo;
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check --features db`
Expected: success. If `ValidatedFilter` doesn't implement `Clone`, adjust by accepting `ValidatedFilter` by value in the `query()` method signature.

- [ ] **Step 4: Commit**

```bash
git add src/audit/repo.rs src/audit/mod.rs
git commit -m "feat(audit): add AuditRepo with paginated query methods"
```

---

### Task 6: Re-exports, Feature Flag, and `src/lib.rs` Wiring

**Files:**
- Modify: `src/audit/mod.rs`
- Modify: `src/lib.rs`
- Modify: `Cargo.toml`

- [ ] **Step 1: Finalize `src/audit/mod.rs` re-exports**

Ensure `src/audit/mod.rs` has all modules and re-exports:

```rust
//! Audit logging for business-significant actions.
//!
//! Provides explicit event recording (no automatic middleware capture) with
//! a pluggable backend trait and a built-in SQLite implementation.
//!
//! | Type | Purpose |
//! |---|---|
//! | [`AuditEntry`] | Builder for audit events — four required fields plus optional metadata, client info, tenant |
//! | [`AuditRecord`] | Stored form returned by queries — all fields flat, includes `id` and `created_at` |
//! | [`AuditLogBackend`] | Object-safe trait for custom storage backends |
//! | [`AuditLog`] | Concrete wrapper — `record()` propagates errors, `record_silent()` traces and swallows |
//! | [`AuditRepo`] | Query interface — dedicated methods plus generic filter-based `query()` |

mod backend;
mod entry;
mod log;
mod record;
mod repo;

pub use backend::AuditLogBackend;
pub use entry::AuditEntry;
pub use self::log::AuditLog;
#[cfg(any(test, feature = "audit-test"))]
pub use self::log::MemoryAuditBackend;
pub use record::AuditRecord;
pub use repo::AuditRepo;
```

- [ ] **Step 2: Add convenience re-exports to `src/lib.rs`**

Add in the re-exports section (near the `pub use error::{Error, Result};` line):

```rust
#[cfg(feature = "db")]
pub use audit::{AuditEntry, AuditLog, AuditLogBackend, AuditRecord, AuditRepo};
```

Also add `ClientInfo` re-export:

```rust
pub use extractor::ClientInfo;
```

- [ ] **Step 3: Add `audit-test` feature to `Cargo.toml`**

In `Cargo.toml` `[features]` section, add:

```toml
audit-test = ["db"]
```

Also add `"audit-test"` to the `full` feature list.

- [ ] **Step 4: Verify compilation**

Run: `cargo check --features db`
Expected: success.

- [ ] **Step 5: Commit**

```bash
git add src/audit/mod.rs src/lib.rs Cargo.toml
git commit -m "feat(audit): add public re-exports, ClientInfo, and audit-test feature flag"
```

---

### Task 7: Integration Tests

**Files:**
- Create: `tests/audit_test.rs`

- [ ] **Step 1: Write integration tests**

In `tests/audit_test.rs`:

```rust
#![cfg(feature = "db")]

use modo::audit::{AuditEntry, AuditLog, AuditRepo};
use modo::db::page::PageRequest;
use modo::extractor::ClientInfo;
use modo::testing::TestDb;

const SCHEMA: &str = "\
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
)";

async fn setup() -> (AuditLog, AuditRepo) {
    let db = TestDb::new().await.exec(SCHEMA).await.db();
    (AuditLog::new(db.clone()), AuditRepo::new(db))
}

fn page(page: i64, per_page: i64) -> PageRequest {
    PageRequest { page, per_page }
}

#[tokio::test]
async fn record_and_read_back() {
    let (log, repo) = setup().await;

    log.record(
        &AuditEntry::new("user_1", "user.created", "user", "usr_abc")
            .metadata(serde_json::json!({"source": "signup"}))
            .client_info(ClientInfo::new().ip("1.2.3.4").user_agent("TestBot/1.0"))
            .tenant_id("t_1"),
    )
    .await
    .unwrap();

    let result = repo.list(&page(1, 10)).await.unwrap();
    assert_eq!(result.total, 1);
    assert_eq!(result.items.len(), 1);

    let record = &result.items[0];
    assert_eq!(record.actor, "user_1");
    assert_eq!(record.action, "user.created");
    assert_eq!(record.resource_type, "user");
    assert_eq!(record.resource_id, "usr_abc");
    assert_eq!(record.metadata["source"], "signup");
    assert_eq!(record.ip.as_deref(), Some("1.2.3.4"));
    assert_eq!(record.user_agent.as_deref(), Some("TestBot/1.0"));
    assert!(record.fingerprint.is_none());
    assert_eq!(record.tenant_id.as_deref(), Some("t_1"));
    assert!(!record.id.is_empty());
    assert!(!record.created_at.is_empty());
}

#[tokio::test]
async fn record_without_optional_fields() {
    let (log, repo) = setup().await;

    log.record(&AuditEntry::new("system", "job.ran", "job", "job_1"))
        .await
        .unwrap();

    let result = repo.list(&page(1, 10)).await.unwrap();
    let record = &result.items[0];
    assert_eq!(record.actor, "system");
    assert_eq!(record.metadata, serde_json::json!({}));
    assert!(record.ip.is_none());
    assert!(record.user_agent.is_none());
    assert!(record.fingerprint.is_none());
    assert!(record.tenant_id.is_none());
}

#[tokio::test]
async fn record_silent_does_not_panic() {
    let (log, _) = setup().await;
    log.record_silent(&AuditEntry::new("system", "test.ok", "test", "t_1"))
        .await;
}

#[tokio::test]
async fn by_actor_filters() {
    let (log, repo) = setup().await;

    log.record(&AuditEntry::new("user_1", "a.1", "x", "x1")).await.unwrap();
    log.record(&AuditEntry::new("user_2", "a.2", "x", "x2")).await.unwrap();
    log.record(&AuditEntry::new("user_1", "a.3", "x", "x3")).await.unwrap();

    let result = repo.by_actor("user_1", &page(1, 10)).await.unwrap();
    assert_eq!(result.total, 2);
    assert!(result.items.iter().all(|r| r.actor == "user_1"));
}

#[tokio::test]
async fn by_resource_filters() {
    let (log, repo) = setup().await;

    log.record(&AuditEntry::new("u", "a", "user", "usr_1")).await.unwrap();
    log.record(&AuditEntry::new("u", "a", "user", "usr_2")).await.unwrap();
    log.record(&AuditEntry::new("u", "a", "doc", "doc_1")).await.unwrap();

    let result = repo.by_resource("user", "usr_1", &page(1, 10)).await.unwrap();
    assert_eq!(result.total, 1);
    assert_eq!(result.items[0].resource_id, "usr_1");
}

#[tokio::test]
async fn by_tenant_filters() {
    let (log, repo) = setup().await;

    log.record(&AuditEntry::new("u", "a", "x", "x1").tenant_id("t_1")).await.unwrap();
    log.record(&AuditEntry::new("u", "a", "x", "x2").tenant_id("t_2")).await.unwrap();

    let result = repo.by_tenant("t_1", &page(1, 10)).await.unwrap();
    assert_eq!(result.total, 1);
    assert_eq!(result.items[0].tenant_id.as_deref(), Some("t_1"));
}

#[tokio::test]
async fn by_action_filters() {
    let (log, repo) = setup().await;

    log.record(&AuditEntry::new("u", "user.created", "user", "u1")).await.unwrap();
    log.record(&AuditEntry::new("u", "user.deleted", "user", "u2")).await.unwrap();

    let result = repo.by_action("user.created", &page(1, 10)).await.unwrap();
    assert_eq!(result.total, 1);
    assert_eq!(result.items[0].action, "user.created");
}

#[tokio::test]
async fn pagination_works() {
    let (log, repo) = setup().await;

    for i in 0..5 {
        log.record(&AuditEntry::new("u", &format!("a.{i}"), "x", &format!("x{i}")))
            .await
            .unwrap();
    }

    let p1 = repo.list(&page(1, 2)).await.unwrap();
    assert_eq!(p1.total, 5);
    assert_eq!(p1.items.len(), 2);
    assert!(p1.has_next);
    assert!(!p1.has_prev);

    let p3 = repo.list(&page(3, 2)).await.unwrap();
    assert_eq!(p3.items.len(), 1);
    assert!(!p3.has_next);
    assert!(p3.has_prev);
}

#[tokio::test]
async fn memory_backend_captures_entries() {
    let (log, backend) = AuditLog::memory();

    log.record(&AuditEntry::new("user_1", "test.action", "test", "t_1"))
        .await
        .unwrap();

    let entries = backend.entries();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].actor(), "user_1");
    assert_eq!(entries[0].action(), "test.action");
}
```

- [ ] **Step 2: Run integration tests**

Run: `cargo test --features test-helpers --test audit_test -- --nocapture`
Expected: all 9 tests pass.

- [ ] **Step 3: Commit**

```bash
git add tests/audit_test.rs
git commit -m "test(audit): add integration tests for record, query, pagination, and memory backend"
```

---

### Task 8: Final Verification

**Files:** None (verification only).

- [ ] **Step 1: Run full test suite**

Run: `cargo test --features test-helpers`
Expected: all existing + new tests pass.

- [ ] **Step 2: Run clippy**

Run: `cargo clippy --features db,test-helpers --tests -- -D warnings`
Expected: no warnings.

- [ ] **Step 3: Run fmt check**

Run: `cargo fmt --check`
Expected: no formatting issues.

- [ ] **Step 4: Verify default feature compilation**

Run: `cargo test`
Expected: all default-feature tests pass (audit module compiles under `db` feature which is default).
