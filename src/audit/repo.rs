use std::sync::Arc;

use crate::db::{
    ConnExt, ConnQueryExt, CursorPage, CursorRequest, Database, Page, PageRequest, ValidatedFilter,
};
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
            vec![libsql::Value::from(actor.to_string())],
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
            vec![
                libsql::Value::from(resource_type.to_string()),
                libsql::Value::from(resource_id.to_string()),
            ],
            req,
        )
        .await
    }

    /// Entries by tenant, newest first.
    pub async fn by_tenant(&self, tenant_id: &str, req: &PageRequest) -> Result<Page<AuditRecord>> {
        self.paginated_where(
            "WHERE tenant_id = ?1",
            vec![libsql::Value::from(tenant_id.to_string())],
            req,
        )
        .await
    }

    /// Entries by action (exact match), newest first.
    pub async fn by_action(&self, action: &str, req: &PageRequest) -> Result<Page<AuditRecord>> {
        self.paginated_where(
            "WHERE action = ?1",
            vec![libsql::Value::from(action.to_string())],
            req,
        )
        .await
    }

    /// Flexible query with a pre-validated filter.
    pub async fn query(
        &self,
        filter: ValidatedFilter,
        req: &PageRequest,
    ) -> Result<Page<AuditRecord>> {
        self.inner
            .db
            .conn()
            .select(&format!("SELECT {COLS} FROM audit_log"))
            .filter(filter)
            .order_by("\"created_at\" DESC")
            .page::<AuditRecord>(req.clone())
            .await
    }

    /// All entries with cursor pagination, newest first.
    ///
    /// Uses keyset pagination on the `id` column for stable, efficient
    /// traversal of large result sets.
    pub async fn list_cursor(&self, req: CursorRequest) -> Result<CursorPage<AuditRecord>> {
        self.inner
            .db
            .conn()
            .select(&format!("SELECT {COLS} FROM audit_log"))
            .cursor::<AuditRecord>(req)
            .await
    }

    /// Flexible query with cursor pagination.
    pub async fn query_cursor(
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

    /// Internal helper: count + fetch with a WHERE clause and explicit params.
    async fn paginated_where(
        &self,
        where_clause: &str,
        params: Vec<libsql::Value>,
        req: &PageRequest,
    ) -> Result<Page<AuditRecord>> {
        let count: i64 = self
            .inner
            .db
            .conn()
            .query_one_map(
                &format!("SELECT COUNT(*) FROM audit_log {where_clause}"),
                params.clone(),
                |row| row.get(0).map_err(Error::from),
            )
            .await?;

        let items: Vec<AuditRecord> = self
            .inner
            .db
            .conn()
            .query_all(
                &format!(
                    "SELECT {COLS} FROM audit_log {where_clause} \
                     ORDER BY \"created_at\" DESC LIMIT {} OFFSET {}",
                    req.per_page,
                    req.offset()
                ),
                params,
            )
            .await?;

        Ok(Page::new(items, count, req.page, req.per_page))
    }
}
