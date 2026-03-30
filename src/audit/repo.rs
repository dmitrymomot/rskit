use std::sync::Arc;

use crate::db::{ConnExt, CursorPage, CursorRequest, Database, ValidatedFilter};
use crate::error::Result;

use super::record::AuditRecord;

const COLS: &str = "id, actor, action, resource_type, resource_id, metadata, \
                    ip, user_agent, fingerprint, tenant_id, created_at";

/// Query interface for audit log records.
///
/// All methods use cursor pagination (keyset on the `id` column, newest
/// first). For filtered queries by actor, resource, tenant, or action,
/// use [`query()`](Self::query) with a [`ValidatedFilter`].
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

    /// All entries, newest first.
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    pub async fn list(&self, req: CursorRequest) -> Result<CursorPage<AuditRecord>> {
        self.inner
            .db
            .conn()
            .select(&format!("SELECT {COLS} FROM audit_log"))
            .cursor::<AuditRecord>(req)
            .await
    }

    /// Flexible query with a pre-validated filter.
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    pub async fn query(
        &self,
        filter: ValidatedFilter,
        req: CursorRequest,
    ) -> Result<CursorPage<AuditRecord>> {
        self.inner
            .db
            .conn()
            .select(&format!("SELECT {COLS} FROM audit_log"))
            .filter(filter)
            .cursor::<AuditRecord>(req)
            .await
    }
}
