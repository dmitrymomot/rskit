use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use axum::extract::FromRequestParts;
use http::request::Parts;
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::error::{Error, HttpError};

use crate::auth::session::data::Session;
use crate::auth::session::meta::SessionMeta;
use crate::auth::session::store::SessionData;
use crate::auth::session::token::SessionToken;

use super::CookieSessionService;

#[derive(Clone)]
pub(crate) enum SessionAction {
    None,
    Set(SessionToken),
    Remove,
}

pub(crate) struct SessionState {
    pub service: CookieSessionService,
    pub meta: SessionMeta,
    pub current: Mutex<Option<SessionData>>,
    pub dirty: AtomicBool,
    pub action: Mutex<SessionAction>,
}

/// Axum extractor providing mutable access to the current cookie-backed session.
///
/// `CookieSession` is inserted into the request extensions by
/// [`super::middleware::CookieSessionLayer`]. Extracting it in a handler does
/// not require the user to be authenticated — call [`CookieSession::current`]
/// to check.
///
/// All read methods are synchronous. Write methods that only modify in-memory
/// data ([`CookieSession::set`], [`CookieSession::remove_key`]) are also
/// synchronous. Methods that touch the database ([`CookieSession::authenticate`],
/// [`CookieSession::logout`], etc.) are `async`.
///
/// # Panics
///
/// Panics if `CookieSessionLayer` is not present in the middleware stack.
pub struct CookieSession {
    pub(crate) state: Arc<SessionState>,
}

impl<S: Send + Sync> FromRequestParts<S> for CookieSession {
    type Rejection = Error;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let state = parts
            .extensions
            .get::<Arc<SessionState>>()
            .cloned()
            .ok_or_else(|| {
                Error::internal("CookieSession requires CookieSessionLayer")
                    .with_code("auth:middleware_missing")
            })?;
        Ok(Self { state })
    }
}

impl CookieSession {
    // --- Synchronous reads ---

    /// Return the loaded session for this request, if authenticated.
    pub fn current(&self) -> Option<Session> {
        let guard = self.state.current.lock().expect("session mutex poisoned");
        guard.as_ref().map(|raw| Session::from(raw.clone()))
    }

    /// Return `true` when a valid, authenticated session exists for this request.
    pub fn is_authenticated(&self) -> bool {
        let guard = self.state.current.lock().expect("session mutex poisoned");
        guard.is_some()
    }

    /// Return the authenticated user's ID, or `None` if no session is active.
    pub fn user_id(&self) -> Option<String> {
        let guard = self.state.current.lock().expect("session mutex poisoned");
        guard.as_ref().map(|s| s.user_id.clone())
    }

    /// Deserialise a value stored in the session under `key`.
    ///
    /// Returns `Ok(None)` when there is no active session or the key is absent.
    ///
    /// # Errors
    ///
    /// Returns an error if the stored value cannot be deserialised into `T`.
    pub fn get<T: DeserializeOwned>(&self, key: &str) -> crate::Result<Option<T>> {
        let guard = self.state.current.lock().expect("session mutex poisoned");
        let session = match guard.as_ref() {
            Some(s) => s,
            None => return Ok(None),
        };
        match session.data.get(key) {
            Some(v) => {
                let val = serde_json::from_value(v.clone()).map_err(|e| {
                    Error::internal(format!("deserialize session key '{key}': {e}"))
                })?;
                Ok(Some(val))
            }
            None => Ok(None),
        }
    }

    // --- In-memory data writes (deferred) ---

    /// Store a serialisable value under `key` in the session data.
    ///
    /// The change is held in memory and flushed to the database by the
    /// middleware after the handler returns. No-op when there is no active
    /// session.
    ///
    /// # Errors
    ///
    /// Returns an error if the value cannot be serialised to JSON.
    pub fn set<T: Serialize>(&self, key: &str, value: &T) -> crate::Result<()> {
        let mut guard = self.state.current.lock().expect("session mutex poisoned");
        let session = match guard.as_mut() {
            Some(s) => s,
            None => return Ok(()), // no-op if no session
        };
        if let serde_json::Value::Object(ref mut map) = session.data {
            map.insert(
                key.to_string(),
                serde_json::to_value(value)
                    .map_err(|e| Error::internal(format!("serialize session value: {e}")))?,
            );
            self.state.dirty.store(true, Ordering::SeqCst);
        }
        Ok(())
    }

    /// Remove a key from the session data.
    ///
    /// No-op when there is no active session or the key does not exist.
    /// The change is flushed to the database by the middleware after the
    /// handler returns.
    pub fn remove_key(&self, key: &str) {
        let mut guard = self.state.current.lock().expect("session mutex poisoned");
        if let Some(ref mut session) = *guard
            && let serde_json::Value::Object(ref mut map) = session.data
            && map.remove(key).is_some()
        {
            self.state.dirty.store(true, Ordering::SeqCst);
        }
    }

    // --- Auth lifecycle (immediate DB writes) ---

    /// Create a new authenticated session for `user_id` with empty data.
    ///
    /// If a session already exists, it is destroyed first (session fixation
    /// prevention). A new token is generated and set on the cookie.
    ///
    /// # Errors
    ///
    /// Returns an error if the existing session cannot be destroyed or the
    /// new session cannot be created in the database.
    pub async fn authenticate(&self, user_id: &str) -> crate::Result<()> {
        self.authenticate_with(user_id, serde_json::json!({})).await
    }

    /// Create a new authenticated session for `user_id` with initial `data`.
    ///
    /// If a session already exists, it is destroyed first (session fixation
    /// prevention). A new token is generated and set on the cookie.
    ///
    /// # Errors
    ///
    /// Returns an error if the existing session cannot be destroyed or the
    /// new session cannot be created in the database.
    pub async fn authenticate_with(
        &self,
        user_id: &str,
        data: serde_json::Value,
    ) -> crate::Result<()> {
        // Destroy current session (fixation prevention)
        let existing_id = {
            let current = self.state.current.lock().expect("session mutex poisoned");
            current.as_ref().map(|s| s.id.clone())
        };
        if let Some(id) = existing_id {
            self.state.service.store().destroy(&id).await?;
        }

        let (session_data, token) = self
            .state
            .service
            .store()
            .create(&self.state.meta, user_id, Some(data))
            .await?;

        *self.state.current.lock().expect("session mutex poisoned") = Some(session_data);
        *self.state.action.lock().expect("session mutex poisoned") = SessionAction::Set(token);
        self.state.dirty.store(false, Ordering::SeqCst);
        Ok(())
    }

    /// Issue a new session token and refresh the session expiry.
    ///
    /// Returns `401 Unauthorized` if there is no active session. Use this
    /// after privilege escalation to prevent session fixation.
    ///
    /// # Errors
    ///
    /// Returns `401 Unauthorized` when no active session exists, or an
    /// internal error if the database update fails.
    pub async fn rotate(&self) -> crate::Result<()> {
        let session_id = {
            let current = self.state.current.lock().expect("session mutex poisoned");
            let session = current
                .as_ref()
                .ok_or_else(|| Error::from(HttpError::Unauthorized))?;
            session.id.clone()
        };

        let store = self.state.service.store();
        let new_token = store.rotate_token(&session_id).await?;

        let now = chrono::Utc::now();
        let new_expires =
            now + chrono::Duration::seconds(self.state.service.config().session_ttl_secs as i64);
        store.touch(&session_id, now, new_expires).await?;

        *self.state.action.lock().expect("session mutex poisoned") = SessionAction::Set(new_token);
        Ok(())
    }

    /// Destroy the current session and clear the session cookie.
    ///
    /// No-op (succeeds silently) when there is no active session.
    ///
    /// # Errors
    ///
    /// Returns an error if the database delete fails.
    pub async fn logout(&self) -> crate::Result<()> {
        let existing_id = {
            let current = self.state.current.lock().expect("session mutex poisoned");
            current.as_ref().map(|s| s.id.clone())
        };
        if let Some(id) = existing_id {
            self.state.service.store().destroy(&id).await?;
        }
        *self.state.action.lock().expect("session mutex poisoned") = SessionAction::Remove;
        *self.state.current.lock().expect("session mutex poisoned") = None;
        Ok(())
    }

    /// Destroy all sessions for the current user and clear the session cookie.
    ///
    /// No-op (succeeds silently) when there is no active session.
    ///
    /// # Errors
    ///
    /// Returns an error if the database delete fails.
    pub async fn logout_all(&self) -> crate::Result<()> {
        let existing_user_id = {
            let current = self.state.current.lock().expect("session mutex poisoned");
            current.as_ref().map(|s| s.user_id.clone())
        };
        if let Some(user_id) = existing_user_id {
            self.state
                .service
                .store()
                .destroy_all_for_user(&user_id)
                .await?;
        }
        *self.state.action.lock().expect("session mutex poisoned") = SessionAction::Remove;
        *self.state.current.lock().expect("session mutex poisoned") = None;
        Ok(())
    }

    /// Destroy all sessions for the current user except the current one.
    ///
    /// Returns `401 Unauthorized` if there is no active session.
    ///
    /// # Errors
    ///
    /// Returns `401 Unauthorized` when no active session exists, or an
    /// internal error if the database delete fails.
    pub async fn logout_other(&self) -> crate::Result<()> {
        let (user_id, session_id) = {
            let current = self.state.current.lock().expect("session mutex poisoned");
            let session = current
                .as_ref()
                .ok_or_else(|| Error::from(HttpError::Unauthorized))?;
            (session.user_id.clone(), session.id.clone())
        };
        self.state
            .service
            .store()
            .destroy_all_except(&user_id, &session_id)
            .await
    }

    /// Return all active sessions for the current user.
    ///
    /// Returns `401 Unauthorized` if there is no active session.
    ///
    /// # Errors
    ///
    /// Returns `401 Unauthorized` when no active session exists, or an
    /// internal error if the database query fails.
    pub async fn list_my_sessions(&self) -> crate::Result<Vec<Session>> {
        let user_id = {
            let current = self.state.current.lock().expect("session mutex poisoned");
            let session = current
                .as_ref()
                .ok_or_else(|| Error::from(HttpError::Unauthorized))?;
            session.user_id.clone()
        };
        self.state.service.list(&user_id).await
    }

    /// Revoke a specific session belonging to the current user.
    ///
    /// Returns `401 Unauthorized` if there is no active session and `404 Not
    /// Found` if `id` does not belong to the current user (deliberately
    /// indistinguishable to prevent enumeration).
    ///
    /// # Errors
    ///
    /// Returns `401 Unauthorized` when no active session exists, `404 Not
    /// Found` when the target session does not exist or belongs to another
    /// user, or an internal error if the database operation fails.
    pub async fn revoke(&self, id: &str) -> crate::Result<()> {
        let current_user_id = {
            let current = self.state.current.lock().expect("session mutex poisoned");
            let session = current
                .as_ref()
                .ok_or_else(|| Error::from(HttpError::Unauthorized))?;
            session.user_id.clone()
        };

        let target = self
            .state
            .service
            .store()
            .read(id)
            .await?
            .ok_or_else(|| Error::from(HttpError::NotFound))?;

        if target.user_id != current_user_id {
            return Err(Error::from(HttpError::NotFound));
        }

        self.state.service.store().destroy(id).await
    }

    // --- Cross-transport operations (delegated to service) ---

    /// List all active sessions for `user_id`.
    ///
    /// Unlike [`list_my_sessions`](Self::list_my_sessions), this method does
    /// not require an authenticated current session — use it from admin
    /// endpoints or after resolving the target user through another channel.
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    pub async fn list(&self, user_id: &str) -> crate::Result<Vec<Session>> {
        self.state.service.list(user_id).await
    }

    /// Revoke a specific session belonging to `user_id` by its ULID.
    ///
    /// Returns `404 auth:session_not_found` when `id` does not exist or
    /// belongs to a different user.
    ///
    /// # Errors
    ///
    /// Returns `404 auth:session_not_found` on ownership mismatch, or an
    /// internal error if the database operation fails.
    pub async fn revoke_by_id(&self, user_id: &str, id: &str) -> crate::Result<()> {
        self.state.service.revoke(user_id, id).await
    }

    /// Revoke all sessions for `user_id`.
    ///
    /// # Errors
    ///
    /// Returns an error if the database delete fails.
    pub async fn revoke_all(&self, user_id: &str) -> crate::Result<()> {
        self.state.service.revoke_all(user_id).await
    }

    /// Revoke all sessions for `user_id` except the one with `keep_id`.
    ///
    /// # Errors
    ///
    /// Returns an error if the database delete fails.
    pub async fn revoke_all_except(&self, user_id: &str, keep_id: &str) -> crate::Result<()> {
        self.state.service.revoke_all_except(user_id, keep_id).await
    }
}
