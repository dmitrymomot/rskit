use crate::config::SessionConfig;
use crate::entity::session::{self, ActiveModel, Column, Entity};
use crate::meta::SessionMeta;
use crate::types::{SessionData, SessionId, SessionToken};
use chrono::{DateTime, Utc};
use modo::Error;
use modo::cookies::CookieConfig;
use modo_db::DbPool;
use modo_db::sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseBackend, EntityTrait, PaginatorTrait, QueryFilter,
    QueryOrder, QuerySelect, Set, TransactionTrait,
};

/// Low-level database-backed session store.
///
/// Handles all CRUD operations on the `modo_sessions` table.  Application code
/// should rarely interact with `SessionStore` directly; use [`crate::SessionManager`]
/// (the axum extractor) for request-scoped session operations instead.
///
/// `SessionStore` is cheaply cloneable and is intended to be registered as a
/// managed service so it can be injected into background jobs.
#[derive(Clone)]
pub struct SessionStore {
    db: DbPool,
    config: SessionConfig,
    cookie_config: CookieConfig,
}

impl SessionStore {
    /// Create a new store backed by `db` with the given session and cookie config.
    pub fn new(db: &DbPool, config: SessionConfig, cookie_config: CookieConfig) -> Self {
        Self {
            db: db.clone(),
            config,
            cookie_config,
        }
    }

    /// Return a reference to the session configuration.
    pub fn config(&self) -> &SessionConfig {
        &self.config
    }

    /// Return a reference to the cookie configuration.
    pub fn cookie_config(&self) -> &CookieConfig {
        &self.cookie_config
    }

    /// Insert a new session for `user_id` and return the persisted [`SessionData`]
    /// together with the plaintext [`SessionToken`] (to be set in the cookie).
    ///
    /// The insert and LRU eviction run inside a single transaction to prevent
    /// race conditions under concurrent logins.
    pub async fn create(
        &self,
        meta: &SessionMeta,
        user_id: &str,
        data: Option<serde_json::Value>,
    ) -> Result<(SessionData, SessionToken), Error> {
        let token = SessionToken::generate();
        let token_hash = token.hash();
        let now = Utc::now();
        let expires_at = now + chrono::Duration::seconds(self.config.session_ttl_secs as i64);
        let data_json = data.unwrap_or(serde_json::json!({}));

        let model = ActiveModel {
            id: Set(SessionId::new().to_string()),
            token_hash: Set(token_hash),
            user_id: Set(user_id.to_string()),
            ip_address: Set(meta.ip_address.clone()),
            user_agent: Set(meta.user_agent.clone()),
            device_name: Set(meta.device_name.clone()),
            device_type: Set(meta.device_type.clone()),
            fingerprint: Set(meta.fingerprint.clone()),
            data: Set(serde_json::to_string(&data_json)
                .map_err(|e| Error::internal(format!("serialize session data: {e}")))?),
            created_at: Set(now),
            last_active_at: Set(now),
            expires_at: Set(expires_at),
        };

        // Wrap insert + enforce in a transaction.
        // SQLite: default BEGIN acquires a write lock on first write (WAL mode),
        //         providing database-level write serialization.
        // Postgres: use SERIALIZABLE isolation to prevent phantom reads
        //           between count and insert under concurrent logins.
        let conn = self.db.connection();
        let txn = if conn.get_database_backend() == DatabaseBackend::Postgres {
            use modo_db::sea_orm::IsolationLevel;
            conn.begin_with_config(Some(IsolationLevel::Serializable), None)
                .await
                .map_err(|e| Error::internal(format!("begin transaction: {e}")))?
        } else {
            conn.begin()
                .await
                .map_err(|e| Error::internal(format!("begin transaction: {e}")))?
        };

        let result = model
            .insert(&txn)
            .await
            .map_err(|e| Error::internal(format!("insert session: {e}")))?;

        self.enforce_session_limit_txn(user_id, &txn).await?;

        txn.commit()
            .await
            .map_err(|e| Error::internal(format!("commit transaction: {e}")))?;

        Ok((model_to_session_data(&result)?, token))
    }

    /// Load a session by its ID.  Returns `None` if not found (does not check
    /// expiry — call [`read_by_token`][Self::read_by_token] for expiry-aware
    /// lookup).
    pub async fn read(&self, id: &SessionId) -> Result<Option<SessionData>, Error> {
        let model = Entity::find_by_id(id.as_str())
            .one(self.db.connection())
            .await
            .map_err(|e| Error::internal(format!("read session: {e}")))?;

        match model {
            Some(m) => Ok(Some(model_to_session_data(&m)?)),
            None => Ok(None),
        }
    }

    /// Load a non-expired session by plaintext token (hashes it internally).
    ///
    /// Returns `None` if no matching, non-expired session is found.
    pub async fn read_by_token(&self, token: &SessionToken) -> Result<Option<SessionData>, Error> {
        let hash = token.hash();
        let model = Entity::find()
            .filter(Column::TokenHash.eq(&hash))
            .filter(Column::ExpiresAt.gt(Utc::now()))
            .one(self.db.connection())
            .await
            .map_err(|e| Error::internal(format!("read session by token: {e}")))?;

        match model {
            Some(m) => Ok(Some(model_to_session_data(&m)?)),
            None => Ok(None),
        }
    }

    /// Delete a session by ID.
    pub async fn destroy(&self, id: &SessionId) -> Result<(), Error> {
        Entity::delete_by_id(id.as_str())
            .exec(self.db.connection())
            .await
            .map_err(|e| Error::internal(format!("destroy session: {e}")))?;
        Ok(())
    }

    /// Replace the token for a session with a newly generated one and return the
    /// new plaintext token.  The session ID and all other fields are unchanged.
    pub async fn rotate_token(&self, id: &SessionId) -> Result<SessionToken, Error> {
        let new_token = SessionToken::generate();
        let new_hash = new_token.hash();

        let model = ActiveModel {
            id: Set(id.as_str().to_string()),
            token_hash: Set(new_hash),
            ..Default::default()
        };

        model
            .update(self.db.connection())
            .await
            .map_err(|e| Error::internal(format!("rotate token: {e}")))?;

        Ok(new_token)
    }

    /// Update `last_active_at` to now and set a new `expires_at` for a session.
    pub async fn touch(&self, id: &SessionId, new_expires_at: DateTime<Utc>) -> Result<(), Error> {
        let model = ActiveModel {
            id: Set(id.as_str().to_string()),
            last_active_at: Set(Utc::now()),
            expires_at: Set(new_expires_at),
            ..Default::default()
        };

        model
            .update(self.db.connection())
            .await
            .map_err(|e| Error::internal(format!("touch session: {e}")))?;

        Ok(())
    }

    /// Replace the JSON payload stored in a session.
    pub async fn update_data(&self, id: &SessionId, data: serde_json::Value) -> Result<(), Error> {
        let model = ActiveModel {
            id: Set(id.as_str().to_string()),
            data: Set(serde_json::to_string(&data)
                .map_err(|e| Error::internal(format!("serialize session data: {e}")))?),
            ..Default::default()
        };

        model
            .update(self.db.connection())
            .await
            .map_err(|e| Error::internal(format!("update session data: {e}")))?;

        Ok(())
    }

    /// Delete all sessions belonging to `user_id`.
    pub async fn destroy_all_for_user(&self, user_id: &str) -> Result<(), Error> {
        Entity::delete_many()
            .filter(Column::UserId.eq(user_id))
            .exec(self.db.connection())
            .await
            .map_err(|e| Error::internal(format!("destroy all sessions for user: {e}")))?;
        Ok(())
    }

    /// Delete all sessions belonging to `user_id` except the one identified by
    /// `keep`.
    pub async fn destroy_all_except(&self, user_id: &str, keep: &SessionId) -> Result<(), Error> {
        Entity::delete_many()
            .filter(Column::UserId.eq(user_id))
            .filter(Column::Id.ne(keep.as_str()))
            .exec(self.db.connection())
            .await
            .map_err(|e| Error::internal(format!("destroy all except: {e}")))?;
        Ok(())
    }

    /// Return all non-expired sessions for `user_id`, ordered by most-recently-active
    /// first.
    pub async fn list_for_user(&self, user_id: &str) -> Result<Vec<SessionData>, Error> {
        let models = Entity::find()
            .filter(Column::UserId.eq(user_id))
            .filter(Column::ExpiresAt.gt(Utc::now()))
            .order_by_desc(Column::LastActiveAt)
            .all(self.db.connection())
            .await
            .map_err(|e| Error::internal(format!("list sessions: {e}")))?;

        models.iter().map(model_to_session_data).collect()
    }

    /// Delete all sessions whose `expires_at` is in the past.
    ///
    /// Returns the number of rows deleted.  Called automatically by the
    /// `cleanup-job` feature's cron job.
    pub async fn cleanup_expired(&self) -> Result<u64, Error> {
        let result = Entity::delete_many()
            .filter(Column::ExpiresAt.lt(Utc::now()))
            .exec(self.db.connection())
            .await
            .map_err(|e| Error::internal(format!("cleanup expired sessions: {e}")))?;
        Ok(result.rows_affected)
    }

    /// Enforce session limit within an existing transaction.
    async fn enforce_session_limit_txn(
        &self,
        user_id: &str,
        txn: &modo_db::sea_orm::DatabaseTransaction,
    ) -> Result<(), Error> {
        let now = Utc::now();

        let count = Entity::find()
            .filter(Column::UserId.eq(user_id))
            .filter(Column::ExpiresAt.gt(now))
            .count(txn)
            .await
            .map_err(|e| Error::internal(format!("count sessions: {e}")))?;

        if count as usize <= self.config.max_sessions_per_user {
            return Ok(());
        }

        let excess = count as usize - self.config.max_sessions_per_user;

        // Find least-recently-used sessions (LRU eviction)
        let oldest = Entity::find()
            .filter(Column::UserId.eq(user_id))
            .filter(Column::ExpiresAt.gt(now))
            .order_by_asc(Column::LastActiveAt)
            .limit(excess as u64)
            .all(txn)
            .await
            .map_err(|e| Error::internal(format!("find oldest sessions: {e}")))?;

        let ids: Vec<String> = oldest.into_iter().map(|m| m.id).collect();
        if !ids.is_empty() {
            Entity::delete_many()
                .filter(Column::Id.is_in(ids))
                .exec(txn)
                .await
                .map_err(|e| Error::internal(format!("evict sessions: {e}")))?;
        }

        Ok(())
    }

    #[allow(dead_code)]
    async fn enforce_session_limit(&self, user_id: &str) -> Result<(), Error> {
        let now = Utc::now();

        let count = Entity::find()
            .filter(Column::UserId.eq(user_id))
            .filter(Column::ExpiresAt.gt(now))
            .count(self.db.connection())
            .await
            .map_err(|e| Error::internal(format!("count sessions: {e}")))?;

        if count as usize <= self.config.max_sessions_per_user {
            return Ok(());
        }

        let excess = count as usize - self.config.max_sessions_per_user;

        // Find least-recently-used sessions (LRU eviction)
        let oldest = Entity::find()
            .filter(Column::UserId.eq(user_id))
            .filter(Column::ExpiresAt.gt(now))
            .order_by_asc(Column::LastActiveAt)
            .limit(excess as u64)
            .all(self.db.connection())
            .await
            .map_err(|e| Error::internal(format!("find oldest sessions: {e}")))?;

        let ids: Vec<String> = oldest.into_iter().map(|m| m.id).collect();
        if !ids.is_empty() {
            Entity::delete_many()
                .filter(Column::Id.is_in(ids))
                .exec(self.db.connection())
                .await
                .map_err(|e| Error::internal(format!("evict sessions: {e}")))?;
        }

        Ok(())
    }
}

fn model_to_session_data(model: &session::Model) -> Result<SessionData, Error> {
    let data: serde_json::Value = serde_json::from_str(&model.data)
        .map_err(|e| Error::internal(format!("deserialize session data: {e}")))?;

    Ok(SessionData {
        id: SessionId::from_raw(&model.id),
        token_hash: model.token_hash.clone(),
        user_id: model.user_id.clone(),
        ip_address: model.ip_address.clone(),
        user_agent: model.user_agent.clone(),
        device_name: model.device_name.clone(),
        device_type: model.device_type.clone(),
        fingerprint: model.fingerprint.clone(),
        data,
        created_at: model.created_at,
        last_active_at: model.last_active_at,
        expires_at: model.expires_at,
    })
}
