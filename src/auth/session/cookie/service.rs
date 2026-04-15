//! `CookieSessionService` — long-lived service for cookie-backed sessions.
//!
//! Holds the session store, cookie signing key, and configuration. Exposes
//! cross-transport operations (`list`, `revoke`, `revoke_all`, etc.) that are
//! used by middleware and state-held service handles.

use std::sync::Arc;

use crate::auth::session::data::Session;
use crate::auth::session::store::SessionStore;
use crate::cookie::{Key, key_from_config};
use crate::db::Database;
use crate::{Error, Result};

use super::CookieSessionsConfig;

/// Long-lived service for cookie-backed sessions.
///
/// `CookieSessionService` wraps a [`SessionStore`], a cookie signing [`Key`],
/// and the full [`CookieSessionsConfig`]. It is constructed once at startup,
/// held in application state, and used by the session middleware and by
/// cross-transport management endpoints.
///
/// # Construction
///
/// ```rust,ignore
/// let svc = CookieSessionService::new(db, config)?;
/// ```
///
/// Construction validates that the cookie secret meets the minimum length
/// requirement and fails fast at startup if it does not.
#[derive(Clone)]
pub struct CookieSessionService {
    inner: Arc<Inner>,
}

struct Inner {
    store: SessionStore,
    config: CookieSessionsConfig,
    cookie_key: Key,
}

impl CookieSessionService {
    /// Construct a new `CookieSessionService`.
    ///
    /// Derives the HMAC signing key from `config.cookie.secret`. Fails if the
    /// secret is shorter than 64 characters.
    ///
    /// # Errors
    ///
    /// Returns [`Error::internal`] if the cookie secret is too short.
    pub fn new(db: Database, config: CookieSessionsConfig) -> Result<Self> {
        let cookie_key = key_from_config(&config.cookie)
            .map_err(|e| Error::internal(format!("cookie key: {e}")))?;
        let store = SessionStore::new(db, config.clone());
        Ok(Self {
            inner: Arc::new(Inner {
                store,
                config,
                cookie_key,
            }),
        })
    }

    /// Return a reference to the underlying session store.
    #[cfg(any(test, feature = "test-helpers"))]
    pub fn store(&self) -> &SessionStore {
        &self.inner.store
    }

    /// Return a reference to the underlying session store (crate-internal).
    #[cfg(not(any(test, feature = "test-helpers")))]
    pub(crate) fn store(&self) -> &SessionStore {
        &self.inner.store
    }

    /// Return a reference to the session configuration.
    pub(crate) fn config(&self) -> &CookieSessionsConfig {
        &self.inner.config
    }

    /// Return a reference to the cookie signing key.
    pub(crate) fn cookie_key(&self) -> &Key {
        &self.inner.cookie_key
    }

    /// List all active (non-expired) sessions for the given user.
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    pub async fn list(&self, user_id: &str) -> Result<Vec<Session>> {
        let raws = self.inner.store.list_for_user(user_id).await?;
        Ok(raws.into_iter().map(Session::from).collect())
    }

    /// Revoke a specific session by its ULID identifier.
    ///
    /// Looks up the session row by `id`, verifies that it belongs to `user_id`,
    /// and destroys it. Returns `404 auth:session_not_found` if the session does
    /// not exist or belongs to a different user.
    ///
    /// # Errors
    ///
    /// Returns `404 auth:session_not_found` on ownership mismatch, or an
    /// internal error if the database operation fails.
    pub async fn revoke(&self, user_id: &str, id: &str) -> Result<()> {
        let row = self.inner.store.read(id).await?.ok_or_else(|| {
            Error::not_found("session not found").with_code("auth:session_not_found")
        })?;

        if row.user_id != user_id {
            return Err(Error::not_found("session not found").with_code("auth:session_not_found"));
        }

        self.inner.store.destroy(id).await
    }

    /// Revoke all sessions for the given user.
    ///
    /// # Errors
    ///
    /// Returns an error if the database delete fails.
    pub async fn revoke_all(&self, user_id: &str) -> Result<()> {
        self.inner.store.destroy_all_for_user(user_id).await
    }

    /// Revoke all sessions for the given user except the one with `keep_id`.
    ///
    /// Used to implement "log out other devices" while keeping the caller's
    /// current session active.
    ///
    /// # Errors
    ///
    /// Returns an error if the database delete fails.
    pub async fn revoke_all_except(&self, user_id: &str, keep_id: &str) -> Result<()> {
        self.inner.store.destroy_all_except(user_id, keep_id).await
    }

    /// Delete all expired sessions from the store.
    ///
    /// Returns the number of rows deleted. Schedule this periodically (e.g.
    /// via a cron job) to keep the `authenticated_sessions` table small.
    ///
    /// # Errors
    ///
    /// Returns an error if the database delete fails.
    pub async fn cleanup_expired(&self) -> Result<u64> {
        self.inner.store.cleanup_expired().await
    }

    /// Build a [`CookieSessionLayer`](super::middleware::CookieSessionLayer) from this service.
    ///
    /// Convenience method so callers can write `service.layer()` instead of
    /// `session::layer(service.clone())`.
    pub fn layer(&self) -> super::middleware::CookieSessionLayer {
        super::middleware::layer(self.clone())
    }
}
