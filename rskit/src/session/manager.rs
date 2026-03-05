use crate::app::AppState;
use crate::error::RskitError;
use crate::session::meta::SessionMeta;
use crate::session::store::SessionStoreDyn;
use crate::session::types::{SessionData, SessionToken};
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub(crate) enum SessionAction {
    None,
    Set(SessionToken),
    Remove,
}

#[derive(Clone)]
pub(crate) struct SessionManagerState {
    pub action: Arc<Mutex<SessionAction>>,
    pub meta: SessionMeta,
    pub store: Arc<dyn SessionStoreDyn>,
    pub current_session: Option<SessionData>,
}

/// High-level session API for handlers.
///
/// Handles login, logout, and session access without exposing cookies or IDs.
/// Requires the session middleware to be applied.
///
/// # Example
/// ```rust,ignore
/// #[handler(POST, "/login")]
/// async fn login(session: SessionManager, db: Db) -> Result<Redirect, RskitError> {
///     let user = verify_credentials(&db, &form).await?;
///     session.authenticate(&user.id).await?;
///     Ok(Redirect::to("/dashboard"))
/// }
/// ```
pub struct SessionManager {
    state: SessionManagerState,
}

impl SessionManager {
    /// Create a session for the given user.
    ///
    /// Destroys any existing session first (fixation prevention).
    /// The session cookie is set automatically on the response.
    pub async fn authenticate(&mut self, user_id: &str) -> Result<(), RskitError> {
        if let Some(ref session) = self.state.current_session {
            self.state.store.destroy(&session.id).await.map_err(|e| {
                tracing::error!(
                    session_id = session.id.as_str(),
                    "Failed to destroy previous session during authentication: {e}"
                );
                RskitError::internal(format!("failed to invalidate previous session: {e}"))
            })?;
        }

        let session_id = self.state.store.create(user_id, &self.state.meta).await?;
        self.state.current_session = self.state.store.read(&session_id).await?;
        let token = self
            .state
            .current_session
            .as_ref()
            .ok_or_else(|| RskitError::internal("session not found after create"))?
            .token
            .clone();
        *self.state.action.lock().unwrap_or_else(|e| e.into_inner()) = SessionAction::Set(token);
        Ok(())
    }

    /// Create a session with custom data attached.
    ///
    /// Same as [`authenticate()`](Self::authenticate) but stores additional JSON data.
    pub async fn authenticate_with(
        &mut self,
        user_id: &str,
        data: serde_json::Value,
    ) -> Result<(), RskitError> {
        if let Some(ref session) = self.state.current_session {
            self.state.store.destroy(&session.id).await.map_err(|e| {
                tracing::error!(
                    session_id = session.id.as_str(),
                    "Failed to destroy previous session during authentication: {e}"
                );
                RskitError::internal(format!("failed to invalidate previous session: {e}"))
            })?;
        }

        let session_id = self
            .state
            .store
            .create_with(user_id, &self.state.meta, data)
            .await?;
        self.state.current_session = self.state.store.read(&session_id).await?;
        let token = self
            .state
            .current_session
            .as_ref()
            .ok_or_else(|| RskitError::internal("session not found after create"))?
            .token
            .clone();
        *self.state.action.lock().unwrap_or_else(|e| e.into_inner()) = SessionAction::Set(token);
        Ok(())
    }

    /// Destroy the current session.
    ///
    /// The session cookie is removed automatically on the response.
    pub async fn logout(&mut self) -> Result<(), RskitError> {
        if let Some(ref session) = self.state.current_session {
            self.state.store.destroy(&session.id).await?;
        }
        *self.state.action.lock().unwrap_or_else(|e| e.into_inner()) = SessionAction::Remove;
        self.state.current_session = None;
        Ok(())
    }

    /// Destroy ALL sessions for the current user.
    ///
    /// The session cookie is removed automatically on the response.
    pub async fn logout_all(&mut self) -> Result<(), RskitError> {
        if let Some(ref session) = self.state.current_session {
            self.state
                .store
                .destroy_all_for_user(&session.user_id)
                .await?;
        }
        *self.state.action.lock().unwrap_or_else(|e| e.into_inner()) = SessionAction::Remove;
        self.state.current_session = None;
        Ok(())
    }

    /// Destroy all sessions for the current user except the current one.
    pub async fn logout_other(&mut self) -> Result<(), RskitError> {
        let session = self
            .state
            .current_session
            .as_ref()
            .ok_or_else(|| RskitError::internal("no active session"))?;
        self.state
            .store
            .destroy_all_except(&session.user_id, &session.id)
            .await?;
        Ok(())
    }

    /// Regenerate the session token without changing the session ID.
    ///
    /// Use after password change or privilege escalation.
    /// The new token is sent to the client via an updated cookie.
    pub async fn rotate(&mut self) -> Result<(), RskitError> {
        let session = self
            .state
            .current_session
            .as_ref()
            .ok_or_else(|| RskitError::internal("no active session"))?;
        let new_token = SessionToken::generate();
        self.state
            .store
            .update_token(&session.id, &new_token)
            .await?;
        *self.state.action.lock().unwrap_or_else(|e| e.into_inner()) =
            SessionAction::Set(new_token.clone());
        self.state
            .current_session
            .as_mut()
            .expect("checked above")
            .token = new_token;
        Ok(())
    }

    /// Access the current session data (if authenticated).
    pub fn current(&self) -> Option<&SessionData> {
        self.state.current_session.as_ref()
    }

    /// Read the current session's data field.
    pub fn data(&self) -> Option<&serde_json::Value> {
        self.state.current_session.as_ref().map(|s| &s.data)
    }

    /// Replace the entire data blob for the current session.
    pub async fn update_data(&mut self, data: serde_json::Value) -> Result<(), RskitError> {
        let session = self
            .state
            .current_session
            .as_ref()
            .ok_or_else(|| RskitError::internal("no active session"))?;
        self.state
            .store
            .update_data(&session.id, data.clone())
            .await?;
        self.state
            .current_session
            .as_mut()
            .expect("checked above")
            .data = data;
        Ok(())
    }

    /// Get a typed value from the session data by key.
    pub fn get<T: DeserializeOwned>(&self, key: &str) -> Option<T> {
        self.state
            .current_session
            .as_ref()
            .and_then(|s| s.data.get(key))
            .and_then(|v| {
                serde_json::from_value(v.clone())
                    .map_err(|e| {
                        tracing::warn!(key, error = %e, "Failed to deserialize session data key — possible type mismatch");
                    })
                    .ok()
            })
    }

    /// Set a single key in the session data (read-modify-write via store).
    pub async fn set(&mut self, key: &str, value: impl Serialize) -> Result<(), RskitError> {
        let session = self
            .state
            .current_session
            .as_ref()
            .ok_or_else(|| RskitError::internal("no active session"))?;

        let mut data = session.data.clone();
        if !data.is_object() {
            data = serde_json::Value::Object(Default::default());
        }
        if let serde_json::Value::Object(ref mut map) = data {
            map.insert(
                key.to_string(),
                serde_json::to_value(value)
                    .map_err(|e| RskitError::internal(format!("serialize session value: {e}")))?,
            );
        }
        self.state
            .store
            .update_data(&session.id, data.clone())
            .await?;
        self.state
            .current_session
            .as_mut()
            .expect("checked above")
            .data = data;
        Ok(())
    }

    /// Remove a key from the session data.
    pub async fn remove_key(&mut self, key: &str) -> Result<(), RskitError> {
        let session = self
            .state
            .current_session
            .as_ref()
            .ok_or_else(|| RskitError::internal("no active session"))?;

        let mut data = session.data.clone();
        if !data.is_object() {
            data = serde_json::Value::Object(Default::default());
        }
        if let serde_json::Value::Object(ref mut map) = data {
            map.remove(key);
        }
        self.state
            .store
            .update_data(&session.id, data.clone())
            .await?;
        self.state
            .current_session
            .as_mut()
            .expect("checked above")
            .data = data;
        Ok(())
    }
}

impl FromRequestParts<AppState> for SessionManager {
    type Rejection = RskitError;

    async fn from_request_parts(
        parts: &mut Parts,
        _state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let state = parts
            .extensions
            .get::<SessionManagerState>()
            .cloned()
            .ok_or_else(|| RskitError::internal("SessionManager requires session middleware"))?;

        Ok(Self { state })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::store::SessionStore;
    use crate::session::types::{SessionId, SessionToken};
    use std::collections::HashMap;
    use std::sync::Mutex as StdMutex;

    struct MemoryStore {
        sessions: StdMutex<HashMap<String, SessionData>>,
    }

    impl MemoryStore {
        fn new() -> Self {
            Self {
                sessions: StdMutex::new(HashMap::new()),
            }
        }
    }

    impl SessionStore for MemoryStore {
        async fn create(&self, user_id: &str, meta: &SessionMeta) -> Result<SessionId, RskitError> {
            let id = SessionId::new();
            let token = SessionToken::generate();
            let session = SessionData {
                id: id.clone(),
                token,
                user_id: user_id.to_string(),
                ip_address: meta.ip_address.clone(),
                user_agent: meta.user_agent.clone(),
                device_name: meta.device_name.clone(),
                device_type: meta.device_type.clone(),
                fingerprint: meta.fingerprint.clone(),
                data: serde_json::json!({}),
                created_at: chrono::Utc::now(),
                last_active_at: chrono::Utc::now(),
                expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
            };
            self.sessions
                .lock()
                .unwrap()
                .insert(id.as_str().to_string(), session);
            Ok(id)
        }

        async fn create_with(
            &self,
            user_id: &str,
            meta: &SessionMeta,
            data: serde_json::Value,
        ) -> Result<SessionId, RskitError> {
            let id = SessionId::new();
            let token = SessionToken::generate();
            let session = SessionData {
                id: id.clone(),
                token,
                user_id: user_id.to_string(),
                ip_address: meta.ip_address.clone(),
                user_agent: meta.user_agent.clone(),
                device_name: meta.device_name.clone(),
                device_type: meta.device_type.clone(),
                fingerprint: meta.fingerprint.clone(),
                data,
                created_at: chrono::Utc::now(),
                last_active_at: chrono::Utc::now(),
                expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
            };
            self.sessions
                .lock()
                .unwrap()
                .insert(id.as_str().to_string(), session);
            Ok(id)
        }

        async fn read(&self, id: &SessionId) -> Result<Option<SessionData>, RskitError> {
            Ok(self.sessions.lock().unwrap().get(id.as_str()).cloned())
        }

        async fn touch(
            &self,
            _id: &SessionId,
            _ttl: std::time::Duration,
        ) -> Result<(), RskitError> {
            Ok(())
        }

        async fn update_data(
            &self,
            id: &SessionId,
            data: serde_json::Value,
        ) -> Result<(), RskitError> {
            let mut sessions = self.sessions.lock().unwrap();
            let session = sessions
                .get_mut(id.as_str())
                .ok_or_else(|| RskitError::internal("session not found"))?;
            session.data = data;
            Ok(())
        }

        async fn destroy(&self, id: &SessionId) -> Result<(), RskitError> {
            self.sessions.lock().unwrap().remove(id.as_str());
            Ok(())
        }

        async fn destroy_all_for_user(&self, user_id: &str) -> Result<(), RskitError> {
            self.sessions
                .lock()
                .unwrap()
                .retain(|_, s| s.user_id != user_id);
            Ok(())
        }

        async fn read_by_token(
            &self,
            token: &SessionToken,
        ) -> Result<Option<SessionData>, RskitError> {
            Ok(self
                .sessions
                .lock()
                .unwrap()
                .values()
                .find(|s| s.token == *token)
                .cloned())
        }

        async fn update_token(
            &self,
            id: &SessionId,
            new_token: &SessionToken,
        ) -> Result<(), RskitError> {
            let mut sessions = self.sessions.lock().unwrap();
            let session = sessions
                .get_mut(id.as_str())
                .ok_or_else(|| RskitError::internal("session not found"))?;
            session.token = new_token.clone();
            Ok(())
        }

        async fn destroy_all_except(
            &self,
            user_id: &str,
            except_id: &SessionId,
        ) -> Result<(), RskitError> {
            self.sessions
                .lock()
                .unwrap()
                .retain(|_, s| s.user_id != user_id || s.id == *except_id);
            Ok(())
        }
    }

    fn test_meta() -> SessionMeta {
        SessionMeta {
            ip_address: "127.0.0.1".to_string(),
            user_agent: "test".to_string(),
            device_name: "test".to_string(),
            device_type: "test".to_string(),
            fingerprint: "abc".to_string(),
        }
    }

    fn make_manager(store: Arc<dyn SessionStoreDyn>) -> SessionManager {
        SessionManager {
            state: SessionManagerState {
                action: Arc::new(Mutex::new(SessionAction::None)),
                meta: test_meta(),
                store,
                current_session: None,
            },
        }
    }

    #[tokio::test]
    async fn logout_clears_current_session() {
        let store = Arc::new(MemoryStore::new()) as Arc<dyn SessionStoreDyn>;
        let mut mgr = make_manager(store);

        mgr.authenticate("user1").await.unwrap();
        assert!(mgr.current().is_some());

        mgr.logout().await.unwrap();
        assert!(
            mgr.current().is_none(),
            "current() should be None after logout"
        );
        assert!(
            mgr.get::<String>("key").is_none(),
            "get() should return None after logout"
        );
    }

    #[tokio::test]
    async fn logout_all_clears_current_session() {
        let store = Arc::new(MemoryStore::new()) as Arc<dyn SessionStoreDyn>;
        let mut mgr = make_manager(store);

        mgr.authenticate("user1").await.unwrap();
        assert!(mgr.current().is_some());

        mgr.logout_all().await.unwrap();
        assert!(
            mgr.current().is_none(),
            "current() should be None after logout_all"
        );
        assert!(
            mgr.get::<String>("key").is_none(),
            "get() should return None after logout_all"
        );
    }

    #[tokio::test]
    async fn logout_other_destroys_other_sessions() {
        let store = Arc::new(MemoryStore::new()) as Arc<dyn SessionStoreDyn>;

        // Create two sessions for the same user via two managers
        let mut mgr1 = make_manager(store.clone());
        mgr1.authenticate("user1").await.unwrap();
        let session1_id = mgr1.current().unwrap().id.clone();

        let mut mgr2 = make_manager(store.clone());
        mgr2.authenticate("user1").await.unwrap();
        let session2_id = mgr2.current().unwrap().id.clone();

        assert_ne!(session1_id, session2_id);

        // logout_other from mgr1 should keep session1, destroy session2
        mgr1.logout_other().await.unwrap();

        // session1 still readable
        assert!(store.read(&session1_id).await.unwrap().is_some());
        // session2 destroyed
        assert!(store.read(&session2_id).await.unwrap().is_none());

        // mgr1's current session is untouched
        assert!(mgr1.current().is_some());
    }

    #[tokio::test]
    async fn rotate_changes_token() {
        let store = Arc::new(MemoryStore::new()) as Arc<dyn SessionStoreDyn>;
        let mut mgr = make_manager(store);

        mgr.authenticate("user1").await.unwrap();
        let old_token = mgr.current().unwrap().token.clone();

        mgr.rotate().await.unwrap();
        let new_token = mgr.current().unwrap().token.clone();

        assert_ne!(old_token, new_token, "token should change after rotate");
        assert_eq!(new_token.as_str().len(), 64, "token should be 64 hex chars");

        // SessionAction should be Set with the new token
        let action = mgr.state.action.lock().unwrap().clone();
        match action {
            SessionAction::Set(t) => assert_eq!(t, new_token),
            _ => panic!("expected SessionAction::Set after rotate"),
        }
    }

    #[tokio::test]
    async fn rotate_without_session_errors() {
        let store = Arc::new(MemoryStore::new()) as Arc<dyn SessionStoreDyn>;
        let mut mgr = make_manager(store);

        let result = mgr.rotate().await;
        assert!(result.is_err(), "rotate without session should error");
    }

    #[tokio::test]
    async fn authenticate_sets_token_in_action() {
        let store = Arc::new(MemoryStore::new()) as Arc<dyn SessionStoreDyn>;
        let mut mgr = make_manager(store);

        mgr.authenticate("user1").await.unwrap();
        let session = mgr.current().unwrap();
        let session_token = session.token.clone();

        // token != session id (they are different concepts)
        assert_ne!(session_token.as_str(), session.id.as_str());

        // SessionAction should be Set with the session token
        let action = mgr.state.action.lock().unwrap().clone();
        match action {
            SessionAction::Set(t) => assert_eq!(t, session_token),
            _ => panic!("expected SessionAction::Set after authenticate"),
        }
    }

    #[tokio::test]
    async fn rotate_invalidates_old_token() {
        let store = Arc::new(MemoryStore::new()) as Arc<dyn SessionStoreDyn>;
        let mut mgr = make_manager(store.clone());

        mgr.authenticate("user1").await.unwrap();
        let old_token = mgr.current().unwrap().token.clone();

        mgr.rotate().await.unwrap();
        let new_token = mgr.current().unwrap().token.clone();

        // Old token should no longer resolve
        assert!(store.read_by_token(&old_token).await.unwrap().is_none());
        // New token should resolve
        assert!(store.read_by_token(&new_token).await.unwrap().is_some());
    }

    #[tokio::test]
    async fn logout_other_preserves_other_users() {
        let store = Arc::new(MemoryStore::new()) as Arc<dyn SessionStoreDyn>;

        // Create session for user2
        let mut mgr2 = make_manager(store.clone());
        mgr2.authenticate("user2").await.unwrap();
        let user2_session_id = mgr2.current().unwrap().id.clone();

        // Create session for user1
        let mut mgr1 = make_manager(store.clone());
        mgr1.authenticate("user1").await.unwrap();

        // logout_other for user1 should not affect user2
        mgr1.logout_other().await.unwrap();

        assert!(
            store.read(&user2_session_id).await.unwrap().is_some(),
            "user2's session should be preserved"
        );
    }

    #[tokio::test]
    async fn logout_other_without_session_errors() {
        let store = Arc::new(MemoryStore::new()) as Arc<dyn SessionStoreDyn>;
        let mut mgr = make_manager(store);

        let result = mgr.logout_other().await;
        assert!(result.is_err(), "logout_other without session should error");
    }

    #[tokio::test]
    async fn logout_other_action_unchanged() {
        let store = Arc::new(MemoryStore::new()) as Arc<dyn SessionStoreDyn>;
        let mut mgr = make_manager(store);

        mgr.authenticate("user1").await.unwrap();
        // Reset action to None (authenticate set it to Set)
        *mgr.state.action.lock().unwrap() = SessionAction::None;

        mgr.logout_other().await.unwrap();

        // Action should still be None — logout_other doesn't touch cookies
        let action = mgr.state.action.lock().unwrap().clone();
        assert!(
            matches!(action, SessionAction::None),
            "logout_other should not change SessionAction"
        );
    }

    // --- 3.2: Session fixation prevention ---

    #[tokio::test]
    async fn authenticate_destroys_old_session() {
        let store = Arc::new(MemoryStore::new()) as Arc<dyn SessionStoreDyn>;
        let mut mgr = make_manager(store.clone());

        // Authenticate as user1
        mgr.authenticate("user1").await.unwrap();
        let old_session_id = mgr.current().unwrap().id.clone();

        // Authenticate as user2 on same manager
        mgr.authenticate("user2").await.unwrap();
        let new_session_id = mgr.current().unwrap().id.clone();

        assert_ne!(old_session_id, new_session_id);
        // Old session must be destroyed in store
        assert!(
            store.read(&old_session_id).await.unwrap().is_none(),
            "old session should be destroyed after re-authenticate"
        );
        // New session belongs to user2
        assert_eq!(mgr.current().unwrap().user_id, "user2");
    }

    // --- 3.3: authenticate_with ---

    #[tokio::test]
    async fn authenticate_with_creates_session_with_data() {
        let store = Arc::new(MemoryStore::new()) as Arc<dyn SessionStoreDyn>;
        let mut mgr = make_manager(store);

        mgr.authenticate_with("user1", serde_json::json!({"role": "admin"}))
            .await
            .unwrap();

        let session = mgr.current().unwrap();
        assert_eq!(session.user_id, "user1");
        assert_eq!(session.data["role"], "admin");
    }

    // --- 3.4: Data methods ---

    #[tokio::test]
    async fn set_persists_to_store_and_in_memory() {
        let store = Arc::new(MemoryStore::new()) as Arc<dyn SessionStoreDyn>;
        let mut mgr = make_manager(store.clone());

        mgr.authenticate("user1").await.unwrap();
        let session_id = mgr.current().unwrap().id.clone();

        mgr.set("theme", "dark").await.unwrap();

        // In-memory
        assert_eq!(mgr.get::<String>("theme").unwrap(), "dark");
        // In store
        let stored = store.read(&session_id).await.unwrap().unwrap();
        assert_eq!(stored.data["theme"], "dark");
    }

    #[tokio::test]
    async fn remove_key_removes_from_store_and_in_memory() {
        let store = Arc::new(MemoryStore::new()) as Arc<dyn SessionStoreDyn>;
        let mut mgr = make_manager(store.clone());

        mgr.authenticate("user1").await.unwrap();
        let session_id = mgr.current().unwrap().id.clone();

        mgr.set("theme", "dark").await.unwrap();
        mgr.remove_key("theme").await.unwrap();

        // In-memory
        assert!(mgr.get::<String>("theme").is_none());
        // In store
        let stored = store.read(&session_id).await.unwrap().unwrap();
        assert!(stored.data.get("theme").is_none());
    }

    #[tokio::test]
    async fn update_data_replaces_blob() {
        let store = Arc::new(MemoryStore::new()) as Arc<dyn SessionStoreDyn>;
        let mut mgr = make_manager(store.clone());

        mgr.authenticate("user1").await.unwrap();
        let session_id = mgr.current().unwrap().id.clone();

        mgr.set("old_key", "value").await.unwrap();
        let new_data = serde_json::json!({"new_key": 42});
        mgr.update_data(new_data.clone()).await.unwrap();

        // In-memory: old_key gone, new_key present
        assert!(mgr.get::<String>("old_key").is_none());
        assert_eq!(mgr.get::<i64>("new_key").unwrap(), 42);
        // In store
        let stored = store.read(&session_id).await.unwrap().unwrap();
        assert_eq!(stored.data, new_data);
    }

    #[tokio::test]
    async fn get_returns_none_on_type_mismatch() {
        let store = Arc::new(MemoryStore::new()) as Arc<dyn SessionStoreDyn>;
        let mut mgr = make_manager(store);

        mgr.authenticate("user1").await.unwrap();
        mgr.set("count", 42i64).await.unwrap();

        // Deserializing an i64 as a Vec<String> should return None
        assert!(mgr.get::<Vec<String>>("count").is_none());
        // Correct type works
        assert_eq!(mgr.get::<i64>("count").unwrap(), 42);
    }

    // --- 3.5: Verify store destruction in logout/logout_all ---

    #[tokio::test]
    async fn logout_destroys_session_in_store() {
        let store = Arc::new(MemoryStore::new()) as Arc<dyn SessionStoreDyn>;
        let mut mgr = make_manager(store.clone());

        mgr.authenticate("user1").await.unwrap();
        let session_id = mgr.current().unwrap().id.clone();

        mgr.logout().await.unwrap();

        assert!(
            store.read(&session_id).await.unwrap().is_none(),
            "session should be removed from store after logout"
        );
    }

    #[tokio::test]
    async fn logout_all_destroys_all_user_sessions_in_store() {
        let store = Arc::new(MemoryStore::new()) as Arc<dyn SessionStoreDyn>;

        // Create two sessions for the same user
        let mut mgr1 = make_manager(store.clone());
        mgr1.authenticate("user1").await.unwrap();
        let session1_id = mgr1.current().unwrap().id.clone();

        let mut mgr2 = make_manager(store.clone());
        mgr2.authenticate("user1").await.unwrap();
        let session2_id = mgr2.current().unwrap().id.clone();

        // logout_all from mgr1
        mgr1.logout_all().await.unwrap();

        assert!(
            store.read(&session1_id).await.unwrap().is_none(),
            "session1 should be removed from store"
        );
        assert!(
            store.read(&session2_id).await.unwrap().is_none(),
            "session2 should be removed from store"
        );
    }

    // --- 3.6: Hard-fail on destroy error ---

    struct FailingDestroyStore {
        inner: MemoryStore,
    }

    impl FailingDestroyStore {
        fn new() -> Self {
            Self {
                inner: MemoryStore::new(),
            }
        }
    }

    impl SessionStore for FailingDestroyStore {
        async fn create(&self, user_id: &str, meta: &SessionMeta) -> Result<SessionId, RskitError> {
            SessionStore::create(&self.inner, user_id, meta).await
        }

        async fn create_with(
            &self,
            user_id: &str,
            meta: &SessionMeta,
            data: serde_json::Value,
        ) -> Result<SessionId, RskitError> {
            SessionStore::create_with(&self.inner, user_id, meta, data).await
        }

        async fn read(&self, id: &SessionId) -> Result<Option<SessionData>, RskitError> {
            SessionStore::read(&self.inner, id).await
        }

        async fn touch(&self, id: &SessionId, ttl: std::time::Duration) -> Result<(), RskitError> {
            SessionStore::touch(&self.inner, id, ttl).await
        }

        async fn update_data(
            &self,
            id: &SessionId,
            data: serde_json::Value,
        ) -> Result<(), RskitError> {
            SessionStore::update_data(&self.inner, id, data).await
        }

        async fn destroy(&self, _id: &SessionId) -> Result<(), RskitError> {
            Err(RskitError::internal("simulated destroy failure"))
        }

        async fn destroy_all_for_user(&self, user_id: &str) -> Result<(), RskitError> {
            SessionStore::destroy_all_for_user(&self.inner, user_id).await
        }

        async fn read_by_token(
            &self,
            token: &SessionToken,
        ) -> Result<Option<SessionData>, RskitError> {
            SessionStore::read_by_token(&self.inner, token).await
        }

        async fn update_token(
            &self,
            id: &SessionId,
            new_token: &SessionToken,
        ) -> Result<(), RskitError> {
            SessionStore::update_token(&self.inner, id, new_token).await
        }

        async fn destroy_all_except(
            &self,
            user_id: &str,
            except_id: &SessionId,
        ) -> Result<(), RskitError> {
            SessionStore::destroy_all_except(&self.inner, user_id, except_id).await
        }
    }

    #[tokio::test]
    async fn authenticate_hard_fails_on_destroy_error() {
        let store = Arc::new(FailingDestroyStore::new()) as Arc<dyn SessionStoreDyn>;
        let mut mgr = make_manager(store.clone());

        // First authenticate succeeds (no prior session to destroy)
        mgr.authenticate("user1").await.unwrap();
        let old_session_id = mgr.current().unwrap().id.clone();

        // Second authenticate should fail because destroy returns error
        let result = mgr.authenticate("user2").await;
        assert!(
            result.is_err(),
            "authenticate should fail when old session destroy fails"
        );

        // Old session should still exist (destroy failed, so it was NOT removed)
        assert!(
            store.read(&old_session_id).await.unwrap().is_some(),
            "old session should still exist since destroy failed"
        );
    }
}
