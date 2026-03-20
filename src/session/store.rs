use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::db::{InnerPool, Reader, Writer};
use crate::error::{Error, Result};

use super::config::SessionConfig;
use super::meta::SessionMeta;
use super::token::SessionToken;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionData {
    pub id: String,
    pub user_id: String,
    pub ip_address: String,
    pub user_agent: String,
    pub device_name: String,
    pub device_type: String,
    pub fingerprint: String,
    pub data: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub last_active_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Clone)]
pub struct Store {
    reader: InnerPool,
    writer: InnerPool,
    config: SessionConfig,
}

impl Store {
    pub fn new(pool: &(impl Reader + Writer), config: SessionConfig) -> Self {
        Self {
            reader: pool.read_pool().clone(),
            writer: pool.write_pool().clone(),
            config,
        }
    }

    pub fn new_rw(reader: &impl Reader, writer: &impl Writer, config: SessionConfig) -> Self {
        Self {
            reader: reader.read_pool().clone(),
            writer: writer.write_pool().clone(),
            config,
        }
    }

    pub fn config(&self) -> &SessionConfig {
        &self.config
    }

    pub async fn read_by_token(&self, token: &SessionToken) -> Result<Option<SessionData>> {
        let hash = token.hash();
        let now = Utc::now().to_rfc3339();
        let row = sqlx::query_as::<_, SessionRow>(
            "SELECT id, user_id, ip_address, user_agent, device_name, device_type, \
             fingerprint, data, created_at, last_active_at, expires_at \
             FROM modo_sessions WHERE token_hash = ? AND expires_at > ?",
        )
        .bind(&hash)
        .bind(&now)
        .fetch_optional(&self.reader)
        .await
        .map_err(|e| Error::internal(format!("read session by token: {e}")))?;

        row.map(row_to_session_data).transpose()
    }

    pub async fn read(&self, id: &str) -> Result<Option<SessionData>> {
        let row = sqlx::query_as::<_, SessionRow>(
            "SELECT id, user_id, ip_address, user_agent, device_name, device_type, \
             fingerprint, data, created_at, last_active_at, expires_at \
             FROM modo_sessions WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.reader)
        .await
        .map_err(|e| Error::internal(format!("read session: {e}")))?;

        row.map(row_to_session_data).transpose()
    }

    pub async fn list_for_user(&self, user_id: &str) -> Result<Vec<SessionData>> {
        let now = Utc::now().to_rfc3339();
        let rows = sqlx::query_as::<_, SessionRow>(
            "SELECT id, user_id, ip_address, user_agent, device_name, device_type, \
             fingerprint, data, created_at, last_active_at, expires_at \
             FROM modo_sessions WHERE user_id = ? AND expires_at > ? \
             ORDER BY last_active_at DESC",
        )
        .bind(user_id)
        .bind(&now)
        .fetch_all(&self.reader)
        .await
        .map_err(|e| Error::internal(format!("list sessions: {e}")))?;

        rows.into_iter().map(row_to_session_data).collect()
    }

    pub async fn create(
        &self,
        meta: &SessionMeta,
        user_id: &str,
        data: Option<serde_json::Value>,
    ) -> Result<(SessionData, SessionToken)> {
        let id = crate::id::ulid();
        let token = SessionToken::generate();
        let token_hash = token.hash();
        let now = Utc::now();
        let expires_at = now + chrono::Duration::seconds(self.config.session_ttl_secs as i64);
        let data_json = data.unwrap_or(serde_json::json!({}));
        let data_str = serde_json::to_string(&data_json)
            .map_err(|e| Error::internal(format!("serialize session data: {e}")))?;
        let now_str = now.to_rfc3339();
        let expires_str = expires_at.to_rfc3339();

        let mut txn = self
            .writer
            .begin()
            .await
            .map_err(|e| Error::internal(format!("begin transaction: {e}")))?;

        sqlx::query(
            "INSERT INTO modo_sessions \
             (id, token_hash, user_id, ip_address, user_agent, device_name, device_type, \
              fingerprint, data, created_at, last_active_at, expires_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&token_hash)
        .bind(user_id)
        .bind(&meta.ip_address)
        .bind(&meta.user_agent)
        .bind(&meta.device_name)
        .bind(&meta.device_type)
        .bind(&meta.fingerprint)
        .bind(&data_str)
        .bind(&now_str)
        .bind(&now_str)
        .bind(&expires_str)
        .execute(&mut *txn)
        .await
        .map_err(|e| Error::internal(format!("insert session: {e}")))?;

        self.enforce_session_limit(user_id, &now_str, &mut txn)
            .await?;

        txn.commit()
            .await
            .map_err(|e| Error::internal(format!("commit transaction: {e}")))?;

        let session_data = SessionData {
            id,
            user_id: user_id.to_string(),
            ip_address: meta.ip_address.clone(),
            user_agent: meta.user_agent.clone(),
            device_name: meta.device_name.clone(),
            device_type: meta.device_type.clone(),
            fingerprint: meta.fingerprint.clone(),
            data: data_json,
            created_at: now,
            last_active_at: now,
            expires_at,
        };

        Ok((session_data, token))
    }

    pub async fn destroy(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM modo_sessions WHERE id = ?")
            .bind(id)
            .execute(&self.writer)
            .await
            .map_err(|e| Error::internal(format!("destroy session: {e}")))?;
        Ok(())
    }

    pub async fn destroy_all_for_user(&self, user_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM modo_sessions WHERE user_id = ?")
            .bind(user_id)
            .execute(&self.writer)
            .await
            .map_err(|e| Error::internal(format!("destroy all sessions for user: {e}")))?;
        Ok(())
    }

    pub async fn destroy_all_except(&self, user_id: &str, keep_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM modo_sessions WHERE user_id = ? AND id != ?")
            .bind(user_id)
            .bind(keep_id)
            .execute(&self.writer)
            .await
            .map_err(|e| Error::internal(format!("destroy all except: {e}")))?;
        Ok(())
    }

    pub async fn rotate_token(&self, id: &str) -> Result<SessionToken> {
        let new_token = SessionToken::generate();
        let new_hash = new_token.hash();
        sqlx::query("UPDATE modo_sessions SET token_hash = ? WHERE id = ?")
            .bind(&new_hash)
            .bind(id)
            .execute(&self.writer)
            .await
            .map_err(|e| Error::internal(format!("rotate token: {e}")))?;
        Ok(new_token)
    }

    pub async fn flush(
        &self,
        id: &str,
        data: &serde_json::Value,
        now: DateTime<Utc>,
        expires_at: DateTime<Utc>,
    ) -> Result<()> {
        let data_str = serde_json::to_string(data)
            .map_err(|e| Error::internal(format!("serialize session data: {e}")))?;
        sqlx::query(
            "UPDATE modo_sessions SET data = ?, last_active_at = ?, expires_at = ? WHERE id = ?",
        )
        .bind(&data_str)
        .bind(now.to_rfc3339())
        .bind(expires_at.to_rfc3339())
        .bind(id)
        .execute(&self.writer)
        .await
        .map_err(|e| Error::internal(format!("flush session: {e}")))?;
        Ok(())
    }

    pub async fn touch(
        &self,
        id: &str,
        now: DateTime<Utc>,
        expires_at: DateTime<Utc>,
    ) -> Result<()> {
        sqlx::query("UPDATE modo_sessions SET last_active_at = ?, expires_at = ? WHERE id = ?")
            .bind(now.to_rfc3339())
            .bind(expires_at.to_rfc3339())
            .bind(id)
            .execute(&self.writer)
            .await
            .map_err(|e| Error::internal(format!("touch session: {e}")))?;
        Ok(())
    }

    pub async fn cleanup_expired(&self) -> Result<u64> {
        let now = Utc::now().to_rfc3339();
        let result = sqlx::query("DELETE FROM modo_sessions WHERE expires_at < ?")
            .bind(&now)
            .execute(&self.writer)
            .await
            .map_err(|e| Error::internal(format!("cleanup expired sessions: {e}")))?;
        Ok(result.rows_affected())
    }

    #[cfg(feature = "sqlite")]
    async fn enforce_session_limit(
        &self,
        user_id: &str,
        now: &str,
        txn: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    ) -> Result<()> {
        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM modo_sessions WHERE user_id = ? AND expires_at > ?",
        )
        .bind(user_id)
        .bind(now)
        .fetch_one(&mut **txn)
        .await
        .map_err(|e| Error::internal(format!("count sessions: {e}")))?;

        let max = self.config.max_sessions_per_user as i64;
        if count.0 <= max {
            return Ok(());
        }

        let excess = count.0 - max;
        let oldest_ids: Vec<(String,)> = sqlx::query_as(
            "SELECT id FROM modo_sessions WHERE user_id = ? AND expires_at > ? \
             ORDER BY last_active_at ASC LIMIT ?",
        )
        .bind(user_id)
        .bind(now)
        .bind(excess)
        .fetch_all(&mut **txn)
        .await
        .map_err(|e| Error::internal(format!("find oldest sessions: {e}")))?;

        for (id,) in oldest_ids {
            sqlx::query("DELETE FROM modo_sessions WHERE id = ?")
                .bind(&id)
                .execute(&mut **txn)
                .await
                .map_err(|e| Error::internal(format!("evict session: {e}")))?;
        }

        Ok(())
    }

    #[cfg(feature = "postgres")]
    async fn enforce_session_limit(
        &self,
        user_id: &str,
        now: &str,
        txn: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ) -> Result<()> {
        let _ = (user_id, now, txn);
        unimplemented!("Postgres session limit enforcement not yet implemented")
    }
}

#[derive(sqlx::FromRow)]
struct SessionRow {
    id: String,
    user_id: String,
    ip_address: String,
    user_agent: String,
    device_name: String,
    device_type: String,
    fingerprint: String,
    data: String,
    created_at: String,
    last_active_at: String,
    expires_at: String,
}

fn row_to_session_data(row: SessionRow) -> Result<SessionData> {
    let data: serde_json::Value = serde_json::from_str(&row.data)
        .map_err(|e| Error::internal(format!("deserialize session data: {e}")))?;
    let created_at = DateTime::parse_from_rfc3339(&row.created_at)
        .map_err(|e| Error::internal(format!("parse created_at: {e}")))?
        .with_timezone(&Utc);
    let last_active_at = DateTime::parse_from_rfc3339(&row.last_active_at)
        .map_err(|e| Error::internal(format!("parse last_active_at: {e}")))?
        .with_timezone(&Utc);
    let expires_at = DateTime::parse_from_rfc3339(&row.expires_at)
        .map_err(|e| Error::internal(format!("parse expires_at: {e}")))?
        .with_timezone(&Utc);

    Ok(SessionData {
        id: row.id,
        user_id: row.user_id,
        ip_address: row.ip_address,
        user_agent: row.user_agent,
        device_name: row.device_name,
        device_type: row.device_type,
        fingerprint: row.fingerprint,
        data,
        created_at,
        last_active_at,
        expires_at,
    })
}
