use serde::Serialize;

use crate::db::{ColumnMap, FromRow};
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
        let metadata: serde_json::Value = serde_json::from_str(&metadata_str).map_err(|e| {
            crate::error::Error::internal(format!("invalid audit metadata JSON: {e}"))
        })?;

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
