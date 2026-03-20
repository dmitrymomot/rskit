use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use axum::extract::FromRequestParts;
use http::request::Parts;
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::error::{Error, HttpError};

use super::meta::SessionMeta;
use super::store::{SessionData, Store};
use super::token::SessionToken;

#[derive(Clone)]
pub(crate) enum SessionAction {
    None,
    Set(SessionToken),
    Remove,
}

pub(crate) struct SessionState {
    pub store: Store,
    pub meta: SessionMeta,
    pub current: Mutex<Option<SessionData>>,
    pub dirty: AtomicBool,
    pub action: Mutex<SessionAction>,
}

pub struct Session {
    state: Arc<SessionState>,
}

impl<S: Send + Sync> FromRequestParts<S> for Session {
    type Rejection = Error;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let state = parts
            .extensions
            .get::<Arc<SessionState>>()
            .cloned()
            .ok_or_else(|| Error::internal("Session extractor requires session middleware"))?;

        Ok(Self { state })
    }
}

impl Session {
    // --- Synchronous reads ---

    pub fn user_id(&self) -> Option<String> {
        let guard = self.state.current.lock().expect("session mutex poisoned");
        guard.as_ref().map(|s| s.user_id.clone())
    }

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

    pub fn is_authenticated(&self) -> bool {
        let guard = self.state.current.lock().expect("session mutex poisoned");
        guard.is_some()
    }

    pub fn current(&self) -> Option<SessionData> {
        let guard = self.state.current.lock().expect("session mutex poisoned");
        guard.clone()
    }

    // --- In-memory data writes (deferred) ---

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

    pub async fn authenticate(&self, user_id: &str) -> crate::Result<()> {
        self.authenticate_with(user_id, serde_json::json!({})).await
    }

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
            self.state.store.destroy(&id).await?;
        }

        let (session_data, token) = self
            .state
            .store
            .create(&self.state.meta, user_id, Some(data))
            .await?;

        *self.state.current.lock().expect("session mutex poisoned") = Some(session_data);
        *self.state.action.lock().expect("session mutex poisoned") = SessionAction::Set(token);
        self.state.dirty.store(false, Ordering::SeqCst);
        Ok(())
    }

    pub async fn rotate(&self) -> crate::Result<()> {
        let session_id = {
            let current = self.state.current.lock().expect("session mutex poisoned");
            let session = current
                .as_ref()
                .ok_or_else(|| Error::from(HttpError::Unauthorized))?;
            session.id.clone()
        };

        let new_token = self.state.store.rotate_token(&session_id).await?;

        let now = chrono::Utc::now();
        let new_expires =
            now + chrono::Duration::seconds(self.state.store.config().session_ttl_secs as i64);
        self.state
            .store
            .touch(&session_id, now, new_expires)
            .await?;

        *self.state.action.lock().expect("session mutex poisoned") = SessionAction::Set(new_token);
        Ok(())
    }

    pub async fn logout(&self) -> crate::Result<()> {
        let existing_id = {
            let current = self.state.current.lock().expect("session mutex poisoned");
            current.as_ref().map(|s| s.id.clone())
        };
        if let Some(id) = existing_id {
            self.state.store.destroy(&id).await?;
        }
        *self.state.action.lock().expect("session mutex poisoned") = SessionAction::Remove;
        *self.state.current.lock().expect("session mutex poisoned") = None;
        Ok(())
    }

    pub async fn logout_all(&self) -> crate::Result<()> {
        let existing_user_id = {
            let current = self.state.current.lock().expect("session mutex poisoned");
            current.as_ref().map(|s| s.user_id.clone())
        };
        if let Some(user_id) = existing_user_id {
            self.state.store.destroy_all_for_user(&user_id).await?;
        }
        *self.state.action.lock().expect("session mutex poisoned") = SessionAction::Remove;
        *self.state.current.lock().expect("session mutex poisoned") = None;
        Ok(())
    }

    pub async fn logout_other(&self) -> crate::Result<()> {
        let (user_id, session_id) = {
            let current = self.state.current.lock().expect("session mutex poisoned");
            let session = current
                .as_ref()
                .ok_or_else(|| Error::from(HttpError::Unauthorized))?;
            (session.user_id.clone(), session.id.clone())
        };
        self.state
            .store
            .destroy_all_except(&user_id, &session_id)
            .await
    }

    pub async fn list_my_sessions(&self) -> crate::Result<Vec<SessionData>> {
        let user_id = {
            let current = self.state.current.lock().expect("session mutex poisoned");
            let session = current
                .as_ref()
                .ok_or_else(|| Error::from(HttpError::Unauthorized))?;
            session.user_id.clone()
        };
        self.state.store.list_for_user(&user_id).await
    }

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
            .store
            .read(id)
            .await?
            .ok_or_else(|| Error::from(HttpError::NotFound))?;

        if target.user_id != current_user_id {
            return Err(Error::from(HttpError::NotFound));
        }

        self.state.store.destroy(id).await
    }
}
