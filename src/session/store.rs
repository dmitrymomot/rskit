use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::db::{InnerPool, Reader, Writer};
use crate::error::{Error, Result};

use super::config::SessionConfig;
use super::meta::SessionMeta;
use super::token::SessionToken;

/// A snapshot of a session row as returned from the database.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionData {
    /// Unique session identifier (ULID).
    pub id: String,
    /// The authenticated user's identifier.
    pub user_id: String,
    /// IP address recorded at login.
    pub ip_address: String,
    /// Raw `User-Agent` header recorded at login.
    pub user_agent: String,
    /// Human-readable device name derived from the user agent (e.g. `"Chrome on macOS"`).
    pub device_name: String,
    /// Device category: `"desktop"`, `"mobile"`, or `"tablet"`.
    pub device_type: String,
    /// SHA-256 fingerprint of the browser environment used to detect session hijacking.
    pub fingerprint: String,
    /// Arbitrary JSON data attached to the session.
    pub data: serde_json::Value,
    /// When the session was created.
    pub created_at: DateTime<Utc>,
    /// When the session was last touched.
    pub last_active_at: DateTime<Utc>,
    /// When the session expires.
    pub expires_at: DateTime<Utc>,
}

/// Low-level SQLite-backed session store.
///
/// Wraps a read pool and a write pool and exposes async methods for all
/// session CRUD operations. Consumed by [`super::middleware::SessionLayer`]
/// and available to handlers via [`super::extractor::Session`].
#[derive(Clone)]
pub struct Store {
    reader: InnerPool,
    writer: InnerPool,
    config: SessionConfig,
}

impl Store {
    /// Create a store using a single pool for both reads and writes.
    ///
    /// Use this when the database connection already combines both roles
    /// (e.g. a local SQLite file or an in-memory pool shared via
    /// `ReadPool::new()` / `WritePool::new()`).
    pub fn new(pool: &(impl Reader + Writer), config: SessionConfig) -> Self {
        Self {
            reader: pool.read_pool().clone(),
            writer: pool.write_pool().clone(),
            config,
        }
    }

    /// Create a store with separate read and write pools.
    pub fn new_rw(reader: &impl Reader, writer: &impl Writer, config: SessionConfig) -> Self {
        Self {
            reader: reader.read_pool().clone(),
            writer: writer.write_pool().clone(),
            config,
        }
    }

    /// Return the session configuration for this store.
    pub fn config(&self) -> &SessionConfig {
        &self.config
    }

    /// Look up an active (non-expired) session by its token hash.
    ///
    /// Returns `None` if no matching session exists or the session has expired.
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

    /// Look up a session by its ULID identifier (ignores expiry).
    ///
    /// Returns `None` if no session with that ID exists.
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

    /// List all active (non-expired) sessions for a user, ordered by most recently active.
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

    /// Create a new session for the given user.
    ///
    /// Runs inside a transaction. After inserting, enforces the
    /// `max_sessions_per_user` limit by evicting the oldest session(s) when
    /// the limit is exceeded.
    ///
    /// Returns the newly-created `SessionData` and the raw `SessionToken` that
    /// must be placed in the cookie.
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

    /// Delete a session by its ULID identifier.
    pub async fn destroy(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM modo_sessions WHERE id = ?")
            .bind(id)
            .execute(&self.writer)
            .await
            .map_err(|e| Error::internal(format!("destroy session: {e}")))?;
        Ok(())
    }

    /// Delete all sessions belonging to a user.
    pub async fn destroy_all_for_user(&self, user_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM modo_sessions WHERE user_id = ?")
            .bind(user_id)
            .execute(&self.writer)
            .await
            .map_err(|e| Error::internal(format!("destroy all sessions for user: {e}")))?;
        Ok(())
    }

    /// Delete all sessions for a user except the one with the given ID.
    ///
    /// Used to implement "log out other devices".
    pub async fn destroy_all_except(&self, user_id: &str, keep_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM modo_sessions WHERE user_id = ? AND id != ?")
            .bind(user_id)
            .bind(keep_id)
            .execute(&self.writer)
            .await
            .map_err(|e| Error::internal(format!("destroy all except: {e}")))?;
        Ok(())
    }

    /// Issue a new token for an existing session, invalidating the old one.
    ///
    /// Returns the new [`SessionToken`]. The middleware will write this token
    /// to the session cookie on the response.
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

    /// Persist the session's JSON data and update `last_active_at` / `expires_at`.
    ///
    /// Called by the middleware at the end of a request when the session was
    /// marked dirty.
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

    /// Update `last_active_at` and `expires_at` without changing session data.
    ///
    /// Called by the middleware when the touch interval has elapsed but the
    /// session data is not dirty.
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

    /// Delete all sessions whose `expires_at` is in the past.
    ///
    /// Returns the number of rows deleted. Schedule this periodically (e.g.
    /// via a cron job) to keep the table small.
    pub async fn cleanup_expired(&self) -> Result<u64> {
        let now = Utc::now().to_rfc3339();
        let result = sqlx::query("DELETE FROM modo_sessions WHERE expires_at < ?")
            .bind(&now)
            .execute(&self.writer)
            .await
            .map_err(|e| Error::internal(format!("cleanup expired sessions: {e}")))?;
        Ok(result.rows_affected())
    }

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
