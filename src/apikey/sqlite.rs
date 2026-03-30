use std::future::Future;
use std::pin::Pin;

use crate::db::{ColumnMap, ConnExt, ConnQueryExt, Database, FromRow};
use crate::error::Result;

use super::backend::ApiKeyBackend;
use super::types::ApiKeyRecord;

pub(crate) struct SqliteBackend {
    db: Database,
}

impl SqliteBackend {
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}

impl FromRow for ApiKeyRecord {
    fn from_row(row: &libsql::Row) -> Result<Self> {
        let cols = ColumnMap::from_row(row);
        let scopes_json: String = cols.get(row, "scopes")?;
        let scopes: Vec<String> = serde_json::from_str(&scopes_json)
            .map_err(|e| crate::Error::internal(format!("deserialize api_keys.scopes: {e}")))?;

        Ok(Self {
            id: cols.get(row, "id")?,
            key_hash: cols.get(row, "key_hash")?,
            tenant_id: cols.get(row, "tenant_id")?,
            name: cols.get(row, "name")?,
            scopes,
            expires_at: cols.get(row, "expires_at")?,
            last_used_at: cols.get(row, "last_used_at")?,
            created_at: cols.get(row, "created_at")?,
            revoked_at: cols.get(row, "revoked_at")?,
        })
    }
}

impl ApiKeyBackend for SqliteBackend {
    fn store(
        &self,
        record: &ApiKeyRecord,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
        let id = record.id.clone();
        let key_hash = record.key_hash.clone();
        let tenant_id = record.tenant_id.clone();
        let name = record.name.clone();
        let scopes = serde_json::to_string(&record.scopes).unwrap_or_else(|_| "[]".into());
        let expires_at = record.expires_at.clone();
        let created_at = record.created_at.clone();

        Box::pin(async move {
            self.db
                .conn()
                .execute_raw(
                    "INSERT INTO api_keys (id, key_hash, tenant_id, name, scopes, expires_at, created_at) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    libsql::params![id, key_hash, tenant_id, name, scopes, expires_at, created_at],
                )
                .await
                .map_err(crate::Error::from)?;
            Ok(())
        })
    }

    fn lookup(
        &self,
        key_id: &str,
    ) -> Pin<Box<dyn Future<Output = Result<Option<ApiKeyRecord>>> + Send + '_>> {
        let key_id = key_id.to_owned();
        Box::pin(async move {
            self.db
                .conn()
                .query_optional::<ApiKeyRecord>(
                    "SELECT id, key_hash, tenant_id, name, scopes, expires_at, \
                            last_used_at, created_at, revoked_at \
                     FROM api_keys WHERE id = ?1",
                    libsql::params![key_id],
                )
                .await
        })
    }

    fn revoke(
        &self,
        key_id: &str,
        revoked_at: &str,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
        let key_id = key_id.to_owned();
        let revoked_at = revoked_at.to_owned();
        Box::pin(async move {
            self.db
                .conn()
                .execute_raw(
                    "UPDATE api_keys SET revoked_at = ?1 WHERE id = ?2",
                    libsql::params![revoked_at, key_id],
                )
                .await
                .map_err(crate::Error::from)?;
            Ok(())
        })
    }

    fn list(
        &self,
        tenant_id: &str,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<ApiKeyRecord>>> + Send + '_>> {
        let tenant_id = tenant_id.to_owned();
        Box::pin(async move {
            self.db
                .conn()
                .query_all::<ApiKeyRecord>(
                    "SELECT id, key_hash, tenant_id, name, scopes, expires_at, \
                            last_used_at, created_at, revoked_at \
                     FROM api_keys WHERE tenant_id = ?1 AND revoked_at IS NULL \
                     ORDER BY created_at DESC",
                    libsql::params![tenant_id],
                )
                .await
        })
    }

    fn update_last_used(
        &self,
        key_id: &str,
        timestamp: &str,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
        let key_id = key_id.to_owned();
        let timestamp = timestamp.to_owned();
        Box::pin(async move {
            self.db
                .conn()
                .execute_raw(
                    "UPDATE api_keys SET last_used_at = ?1 WHERE id = ?2",
                    libsql::params![timestamp, key_id],
                )
                .await
                .map_err(crate::Error::from)?;
            Ok(())
        })
    }

    fn update_expires_at(
        &self,
        key_id: &str,
        expires_at: Option<&str>,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
        let key_id = key_id.to_owned();
        let expires_at = expires_at.map(|s| s.to_owned());
        Box::pin(async move {
            self.db
                .conn()
                .execute_raw(
                    "UPDATE api_keys SET expires_at = ?1 WHERE id = ?2",
                    libsql::params![expires_at, key_id],
                )
                .await
                .map_err(crate::Error::from)?;
            Ok(())
        })
    }
}
