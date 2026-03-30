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

    /// Record an audit event, propagating errors via `Result`.
    ///
    /// # Errors
    ///
    /// Returns an error if the backend write fails (e.g. database
    /// connection lost, constraint violation).
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
    fn record<'a>(
        &'a self,
        entry: &'a AuditEntry,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            let id = id::ulid();
            let metadata_json = entry
                .metadata_value()
                .map(|v| v.to_string())
                .unwrap_or_else(|| "{}".to_string());

            let (ip, user_agent, fingerprint) = match entry.client_info_value() {
                Some(ci) => (
                    ci.ip_value().map(String::from),
                    ci.user_agent_value().map(String::from),
                    ci.fingerprint_value().map(String::from),
                ),
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
    fn record<'a>(
        &'a self,
        entry: &'a AuditEntry,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        self.entries.lock().unwrap().push(entry.clone());
        Box::pin(async { Ok(()) })
    }
}
