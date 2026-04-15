//! Low-level SQLite-backed session store — internal to `auth::session`.
//!
//! [`SessionStore`] is `pub(crate)` in normal builds and exposed as `pub` only
//! under `#[cfg(any(test, feature = "test-helpers"))]`. Application code should
//! interact with sessions through [`super::cookie::CookieSessionService`] or
//! [`super::jwt::JwtSessionService`] rather than this type directly.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::db::{ColumnMap, ConnExt, ConnQueryExt, Database, FromRow};
use crate::error::{Error, Result};

use super::cookie::CookieSessionsConfig;
use super::meta::SessionMeta;
use super::token::SessionToken;

const SESSION_COLUMNS: &str = "id, user_id, ip_address, user_agent, device_name, device_type, \
    fingerprint, data, created_at, last_active_at, expires_at";

const TABLE: &str = "authenticated_sessions";

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
/// Wraps a [`Database`] handle and exposes async methods for all session CRUD
/// operations. Consumed by [`super::cookie::CookieSessionLayer`] and
/// [`super::jwt::JwtLayer`]; session data is exposed to handlers via
/// [`super::data::Session`].
#[derive(Clone)]
pub struct SessionStore {
    db: Database,
    config: CookieSessionsConfig,
}

impl SessionStore {
    /// Create a store from a [`Database`] handle and session configuration.
    pub fn new(db: Database, config: CookieSessionsConfig) -> Self {
        Self { db, config }
    }

    /// Return the session configuration for this store.
    pub fn config(&self) -> &CookieSessionsConfig {
        &self.config
    }

    /// Look up an active (non-expired) session by its token hash.
    ///
    /// Returns `None` if no matching session exists or the session has expired.
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails or the stored data cannot
    /// be deserialised.
    pub async fn read_by_token(&self, token: &SessionToken) -> Result<Option<SessionData>> {
        let hash = token.hash();
        let now = Utc::now().to_rfc3339();
        let row: Option<SessionRow> = self
            .db
            .conn()
            .query_optional(
                &format!(
                    "SELECT {SESSION_COLUMNS} FROM {TABLE} \
                     WHERE session_token_hash = ?1 AND expires_at > ?2"
                ),
                libsql::params![hash, now],
            )
            .await?;

        row.map(row_to_session_data).transpose()
    }

    /// Look up a session by its ULID identifier (ignores expiry).
    ///
    /// Returns `None` if no session with that ID exists.
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails or the stored data cannot
    /// be deserialised.
    pub async fn read(&self, id: &str) -> Result<Option<SessionData>> {
        let row: Option<SessionRow> = self
            .db
            .conn()
            .query_optional(
                &format!("SELECT {SESSION_COLUMNS} FROM {TABLE} WHERE id = ?1"),
                libsql::params![id],
            )
            .await?;

        row.map(row_to_session_data).transpose()
    }

    /// List all active (non-expired) sessions for a user, ordered by most recently active.
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails or the stored data cannot
    /// be deserialised.
    pub async fn list_for_user(&self, user_id: &str) -> Result<Vec<SessionData>> {
        let now = Utc::now().to_rfc3339();
        let rows: Vec<SessionRow> = self
            .db
            .conn()
            .query_all(
                &format!(
                    "SELECT {SESSION_COLUMNS} FROM {TABLE} \
                     WHERE user_id = ?1 AND expires_at > ?2 \
                     ORDER BY last_active_at DESC"
                ),
                libsql::params![user_id, now],
            )
            .await?;

        rows.into_iter().map(row_to_session_data).collect()
    }

    /// Create a new session for the given user.
    ///
    /// Inserts the session row then trims excess sessions when the
    /// `max_sessions_per_user` limit is exceeded by evicting the oldest
    /// session(s).
    ///
    /// Returns the newly-created `SessionData` and the raw `SessionToken` that
    /// must be placed in the cookie.
    ///
    /// # Errors
    ///
    /// Returns an error if the session data cannot be serialised or the
    /// database insert/eviction query fails.
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

        // Insert session
        self.db
            .conn()
            .execute_raw(
                &format!(
                    "INSERT INTO {TABLE} \
                     (id, session_token_hash, user_id, ip_address, user_agent, device_name, \
                      device_type, fingerprint, data, created_at, last_active_at, expires_at) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)"
                ),
                libsql::params![
                    id.as_str(),
                    token_hash.as_str(),
                    user_id,
                    meta.ip_address.as_str(),
                    meta.user_agent.as_str(),
                    meta.device_name.as_str(),
                    meta.device_type.as_str(),
                    meta.fingerprint.as_str(),
                    data_str.as_str(),
                    now_str.as_str(),
                    now_str.as_str(),
                    expires_str.as_str()
                ],
            )
            .await
            .map_err(|e| Error::internal(format!("insert session: {e}")))?;

        // Trim excess sessions
        let max = self.config.max_sessions_per_user as i64;
        self.db
            .conn()
            .execute_raw(
                &format!(
                    "DELETE FROM {TABLE} WHERE id IN (\
                         SELECT id FROM {TABLE} \
                         WHERE user_id = ?1 AND expires_at > ?2 \
                         ORDER BY last_active_at ASC \
                         LIMIT MAX(0, (SELECT COUNT(*) FROM {TABLE} \
                                       WHERE user_id = ?3 AND expires_at > ?4) - ?5)\
                     )"
                ),
                libsql::params![user_id, now_str.as_str(), user_id, now_str.as_str(), max],
            )
            .await
            .map_err(|e| Error::internal(format!("evict excess sessions: {e}")))?;

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
    ///
    /// # Errors
    ///
    /// Returns an error if the database delete fails.
    pub async fn destroy(&self, id: &str) -> Result<()> {
        self.db
            .conn()
            .execute_raw(
                &format!("DELETE FROM {TABLE} WHERE id = ?1"),
                libsql::params![id],
            )
            .await
            .map_err(|e| Error::internal(format!("destroy session: {e}")))?;
        Ok(())
    }

    /// Delete all sessions belonging to a user.
    ///
    /// # Errors
    ///
    /// Returns an error if the database delete fails.
    pub async fn destroy_all_for_user(&self, user_id: &str) -> Result<()> {
        self.db
            .conn()
            .execute_raw(
                &format!("DELETE FROM {TABLE} WHERE user_id = ?1"),
                libsql::params![user_id],
            )
            .await
            .map_err(|e| Error::internal(format!("destroy all sessions for user: {e}")))?;
        Ok(())
    }

    /// Delete all sessions for a user except the one with the given ID.
    ///
    /// Used to implement "log out other devices".
    ///
    /// # Errors
    ///
    /// Returns an error if the database delete fails.
    pub async fn destroy_all_except(&self, user_id: &str, keep_id: &str) -> Result<()> {
        self.db
            .conn()
            .execute_raw(
                &format!("DELETE FROM {TABLE} WHERE user_id = ?1 AND id != ?2"),
                libsql::params![user_id, keep_id],
            )
            .await
            .map_err(|e| Error::internal(format!("destroy all except: {e}")))?;
        Ok(())
    }

    /// Look up an active (non-expired) session directly by the stored token hash.
    ///
    /// Unlike [`read_by_token`](Self::read_by_token), this variant accepts a
    /// pre-computed hash string, which is needed by the JWT session service that
    /// stores the token hex as the JWT `jti` and computes the hash itself.
    ///
    /// Returns `None` if no matching session exists or the session has expired.
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails or the stored data cannot
    /// be deserialised.
    pub async fn read_by_token_hash(&self, hash: &str) -> Result<Option<SessionData>> {
        let now = Utc::now().to_rfc3339();
        let row: Option<SessionRow> = self
            .db
            .conn()
            .query_optional(
                &format!(
                    "SELECT {SESSION_COLUMNS} FROM {TABLE} \
                     WHERE session_token_hash = ?1 AND expires_at > ?2"
                ),
                libsql::params![hash, now],
            )
            .await?;

        row.map(row_to_session_data).transpose()
    }

    /// Rotate the session token to a caller-supplied new token.
    ///
    /// Updates `session_token_hash` and `last_active_at` for the session with
    /// the given `id`. Used by the JWT session service, which needs to control
    /// the new token value (so the `jti` in the refresh JWT matches the stored
    /// hash).
    ///
    /// # Errors
    ///
    /// Returns an error if the database update fails.
    pub async fn rotate_token_to(&self, id: &str, new_token: &SessionToken) -> Result<()> {
        let new_hash = new_token.hash();
        let now = Utc::now().to_rfc3339();
        self.db
            .conn()
            .execute_raw(
                &format!(
                    "UPDATE {TABLE} SET session_token_hash = ?1, last_active_at = ?2 WHERE id = ?3"
                ),
                libsql::params![new_hash, now, id],
            )
            .await
            .map_err(|e| Error::internal(format!("rotate token to: {e}")))?;
        Ok(())
    }

    /// Issue a new token for an existing session, invalidating the old one.
    ///
    /// Updates `session_token_hash` and `last_active_at` in a single statement.
    /// Returns the new [`SessionToken`]. The middleware will write this token
    /// to the session cookie on the response.
    ///
    /// # Errors
    ///
    /// Returns an error if the database update fails.
    pub async fn rotate_token(&self, id: &str) -> Result<SessionToken> {
        let new_token = SessionToken::generate();
        let new_hash = new_token.hash();
        let now = Utc::now().to_rfc3339();
        self.db
            .conn()
            .execute_raw(
                &format!(
                    "UPDATE {TABLE} SET session_token_hash = ?1, last_active_at = ?2 WHERE id = ?3"
                ),
                libsql::params![new_hash, now, id],
            )
            .await
            .map_err(|e| Error::internal(format!("rotate token: {e}")))?;
        Ok(new_token)
    }

    /// Persist the session's JSON data and update `last_active_at` / `expires_at`.
    ///
    /// Called by the middleware at the end of a request when the session was
    /// marked dirty.
    ///
    /// # Errors
    ///
    /// Returns an error if the session data cannot be serialised or the
    /// database update fails.
    pub async fn flush(
        &self,
        id: &str,
        data: &serde_json::Value,
        now: DateTime<Utc>,
        expires_at: DateTime<Utc>,
    ) -> Result<()> {
        let data_str = serde_json::to_string(data)
            .map_err(|e| Error::internal(format!("serialize session data: {e}")))?;
        self.db
            .conn()
            .execute_raw(
                &format!(
                    "UPDATE {TABLE} SET data = ?1, last_active_at = ?2, expires_at = ?3 \
                     WHERE id = ?4"
                ),
                libsql::params![data_str, now.to_rfc3339(), expires_at.to_rfc3339(), id],
            )
            .await
            .map_err(|e| Error::internal(format!("flush session: {e}")))?;
        Ok(())
    }

    /// Update `last_active_at` and `expires_at` without changing session data.
    ///
    /// Called by the middleware when the touch interval has elapsed but the
    /// session data is not dirty.
    ///
    /// # Errors
    ///
    /// Returns an error if the database update fails.
    pub async fn touch(
        &self,
        id: &str,
        now: DateTime<Utc>,
        expires_at: DateTime<Utc>,
    ) -> Result<()> {
        self.db
            .conn()
            .execute_raw(
                &format!("UPDATE {TABLE} SET last_active_at = ?1, expires_at = ?2 WHERE id = ?3"),
                libsql::params![now.to_rfc3339(), expires_at.to_rfc3339(), id],
            )
            .await
            .map_err(|e| Error::internal(format!("touch session: {e}")))?;
        Ok(())
    }

    /// Delete all sessions whose `expires_at` is in the past.
    ///
    /// Returns the number of rows deleted. Schedule this periodically (e.g.
    /// via a cron job) to keep the table small.
    ///
    /// # Errors
    ///
    /// Returns an error if the database delete fails.
    #[allow(dead_code)]
    pub async fn cleanup_expired(&self) -> Result<u64> {
        let now = Utc::now().to_rfc3339();
        let affected = self
            .db
            .conn()
            .execute_raw(
                &format!("DELETE FROM {TABLE} WHERE expires_at < ?1"),
                libsql::params![now],
            )
            .await
            .map_err(Error::from)?;
        Ok(affected)
    }
}

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

impl FromRow for SessionRow {
    fn from_row(row: &libsql::Row) -> Result<Self> {
        let cols = ColumnMap::from_row(row);
        Ok(Self {
            id: cols.get(row, "id")?,
            user_id: cols.get(row, "user_id")?,
            ip_address: cols.get(row, "ip_address")?,
            user_agent: cols.get(row, "user_agent")?,
            device_name: cols.get(row, "device_name")?,
            device_type: cols.get(row, "device_type")?,
            fingerprint: cols.get(row, "fingerprint")?,
            data: cols.get(row, "data")?,
            created_at: cols.get(row, "created_at")?,
            last_active_at: cols.get(row, "last_active_at")?,
            expires_at: cols.get(row, "expires_at")?,
        })
    }
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
