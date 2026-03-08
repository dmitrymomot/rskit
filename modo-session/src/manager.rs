use crate::middleware::{SessionAction, SessionManagerState};
use crate::types::{SessionData, SessionId};
use modo::axum::extract::FromRequestParts;
use modo::axum::http::request::Parts;
use modo::{Error, HttpError};
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::sync::Arc;

pub struct SessionManager {
    state: Arc<SessionManagerState>,
}

impl<S: Send + Sync> FromRequestParts<S> for SessionManager {
    type Rejection = Error;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let state = parts
            .extensions
            .get::<Arc<SessionManagerState>>()
            .cloned()
            .ok_or_else(|| Error::internal("SessionManager requires session middleware"))?;

        Ok(Self { state })
    }
}

impl SessionManager {
    /// Create a session for the given user.
    /// Destroys any existing session first (fixation prevention).
    pub async fn authenticate(&self, user_id: &str) -> Result<(), Error> {
        self.authenticate_with(user_id, serde_json::json!({})).await
    }

    /// Create a session with custom data attached.
    pub async fn authenticate_with(
        &self,
        user_id: &str,
        data: serde_json::Value,
    ) -> Result<(), Error> {
        // Destroy current session (fixation prevention)
        {
            let current = self.state.current_session.lock().await;
            if let Some(ref session) = *current {
                self.state.store.destroy(&session.id).await.map_err(|e| {
                    tracing::error!(
                        session_id = session.id.as_str(),
                        "Failed to destroy previous session during authentication: {e}"
                    );
                    Error::internal(format!("failed to invalidate previous session: {e}"))
                })?;
            }
        }

        let (session_data, token) = self
            .state
            .store
            .create(&self.state.meta, user_id, Some(data))
            .await?;

        *self.state.current_session.lock().await = Some(session_data);
        *self.state.action.lock().await = SessionAction::Set(token);
        Ok(())
    }

    /// Destroy the current session. Cookie is removed automatically.
    pub async fn logout(&self) -> Result<(), Error> {
        {
            let current = self.state.current_session.lock().await;
            if let Some(ref session) = *current {
                self.state.store.destroy(&session.id).await?;
            }
        }
        *self.state.action.lock().await = SessionAction::Remove;
        *self.state.current_session.lock().await = None;
        Ok(())
    }

    /// Destroy ALL sessions for the current user.
    pub async fn logout_all(&self) -> Result<(), Error> {
        {
            let current = self.state.current_session.lock().await;
            if let Some(ref session) = *current {
                self.state
                    .store
                    .destroy_all_for_user(&session.user_id)
                    .await?;
            }
        }
        *self.state.action.lock().await = SessionAction::Remove;
        *self.state.current_session.lock().await = None;
        Ok(())
    }

    /// Destroy all sessions for the current user except the current one.
    pub async fn logout_other(&self) -> Result<(), Error> {
        let current = self.state.current_session.lock().await;
        let session = current
            .as_ref()
            .ok_or_else(|| Error::from(HttpError::Unauthorized))?;
        self.state
            .store
            .destroy_all_except(&session.user_id, &session.id)
            .await
    }

    /// Destroy a specific session by ID (for "manage my devices" UI).
    /// Only works on sessions owned by the current user.
    pub async fn revoke(&self, id: &SessionId) -> Result<(), Error> {
        let current = self.state.current_session.lock().await;
        let session = current
            .as_ref()
            .ok_or_else(|| Error::from(HttpError::Unauthorized))?;

        let target = self
            .state
            .store
            .read(id)
            .await?
            .ok_or_else(|| Error::from(HttpError::NotFound))?;

        if target.user_id != session.user_id {
            return Err(Error::from(HttpError::NotFound));
        }

        self.state.store.destroy(id).await
    }

    /// Regenerate the session token without changing the session ID.
    pub async fn rotate(&self) -> Result<(), Error> {
        let session_id = {
            let current = self.state.current_session.lock().await;
            let session = current
                .as_ref()
                .ok_or_else(|| Error::from(HttpError::Unauthorized))?;
            session.id.clone()
        };

        let new_token = self.state.store.rotate_token(&session_id).await?;

        {
            let mut current = self.state.current_session.lock().await;
            if let Some(ref mut s) = *current {
                s.token_hash = new_token.hash();
            }
        }

        *self.state.action.lock().await = SessionAction::Set(new_token);
        Ok(())
    }

    /// Access the current session data (if authenticated).
    pub async fn current(&self) -> Option<SessionData> {
        self.state.current_session.lock().await.clone()
    }

    /// Get the current user ID.
    pub async fn user_id(&self) -> Option<String> {
        self.state
            .current_session
            .lock()
            .await
            .as_ref()
            .map(|s| s.user_id.clone())
    }

    /// Check if a session is active.
    pub async fn is_authenticated(&self) -> bool {
        self.state.current_session.lock().await.is_some()
    }

    /// List all active sessions for the authenticated user.
    pub async fn list_my_sessions(&self) -> Result<Vec<SessionData>, Error> {
        let user_id = {
            let current = self.state.current_session.lock().await;
            let session = current
                .as_ref()
                .ok_or_else(|| Error::from(HttpError::Unauthorized))?;
            session.user_id.clone()
        };
        self.state.store.list_for_user(&user_id).await
    }

    /// Get a typed value from the session data by key.
    pub async fn get<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>, Error> {
        let current = self.state.current_session.lock().await;
        let session = match current.as_ref() {
            Some(s) => s,
            None => return Ok(None),
        };
        match session.data.get(key) {
            Some(v) => match serde_json::from_value(v.clone()) {
                Ok(val) => Ok(Some(val)),
                Err(e) => {
                    tracing::warn!(key, error = %e, "Failed to deserialize session data key");
                    Ok(None)
                }
            },
            None => Ok(None),
        }
    }

    /// Set a single key in the session data (immediate DB write).
    pub async fn set<T: Serialize>(&self, key: &str, value: &T) -> Result<(), Error> {
        let mut current = self.state.current_session.lock().await;
        let session = current
            .as_mut()
            .ok_or_else(|| Error::from(HttpError::Unauthorized))?;

        if !session.data.is_object() {
            session.data = serde_json::Value::Object(Default::default());
        }
        if let serde_json::Value::Object(ref mut map) = session.data {
            map.insert(
                key.to_string(),
                serde_json::to_value(value)
                    .map_err(|e| Error::internal(format!("serialize session value: {e}")))?,
            );
        }
        self.state
            .store
            .update_data(&session.id, session.data.clone())
            .await
    }

    /// Remove a key from the session data (immediate DB write).
    pub async fn remove_key(&self, key: &str) -> Result<(), Error> {
        let mut current = self.state.current_session.lock().await;
        let session = current
            .as_mut()
            .ok_or_else(|| Error::from(HttpError::Unauthorized))?;

        if let serde_json::Value::Object(ref mut map) = session.data {
            map.remove(key);
        }
        self.state
            .store
            .update_data(&session.id, session.data.clone())
            .await
    }
}
