use axum::body::Body;
use axum::http::{Request, StatusCode};
use rskit::app::AppState;
use rskit::error::RskitError;
use rskit::router::RouteRegistration;
use tower::ServiceExt;

#[rskit::handler(GET, "/test")]
async fn test_handler() -> &'static str {
    "test response"
}

#[rskit::handler(GET, "/test/error")]
async fn test_error() -> Result<&'static str, RskitError> {
    Err(RskitError::NotFound)
}

fn build_test_router() -> axum::Router {
    let state = AppState {
        db: None,
        services: Default::default(),
        config: rskit::config::AppConfig::default(),
        cookie_key: axum_extra::extract::cookie::Key::generate(),
        session_store: None,
    };

    let mut router = axum::Router::new();
    for reg in inventory::iter::<RouteRegistration> {
        if reg.path.starts_with("/test") {
            let method_router = (reg.handler)();
            router = router.route(reg.path, method_router);
        }
    }
    router.with_state(state)
}

#[tokio::test]
async fn test_get_handler_returns_200() {
    let app = build_test_router();

    let response = app
        .oneshot(Request::builder().uri("/test").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(&body[..], b"test response");
}

#[tokio::test]
async fn test_error_handler_returns_404_json() {
    let app = build_test_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/test/error")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], 404);
    assert_eq!(json["error"], "Not found");
}

#[tokio::test]
async fn test_unknown_route_returns_404() {
    let app = build_test_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/nonexistent")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

// ============================================================================
// Session middleware & auth extractor integration tests
// ============================================================================

mod session_integration {
    use super::*;
    use axum::http::header;
    use axum::response::IntoResponse;
    use axum::routing::{get, post};
    use rskit::app::ServiceRegistry;
    use rskit::extractors::auth::{Auth, OptionalAuth, UserProvider, UserProviderService};
    use rskit::session::{
        SessionData, SessionId, SessionManager, SessionMeta, SessionStore, SessionStoreDyn,
        SessionToken,
    };
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    // --- Test MemoryStore ---

    struct MemoryStore {
        sessions: Mutex<HashMap<String, SessionData>>,
    }

    impl MemoryStore {
        fn new() -> Self {
            Self {
                sessions: Mutex::new(HashMap::new()),
            }
        }

        fn get_session(&self, id: &SessionId) -> Option<SessionData> {
            self.sessions.lock().unwrap().get(id.as_str()).cloned()
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

    // --- FailingReadStore for transient DB error test ---

    struct FailingReadStore;

    impl SessionStore for FailingReadStore {
        async fn create(
            &self,
            _user_id: &str,
            _meta: &SessionMeta,
        ) -> Result<SessionId, RskitError> {
            unimplemented!()
        }
        async fn create_with(
            &self,
            _user_id: &str,
            _meta: &SessionMeta,
            _data: serde_json::Value,
        ) -> Result<SessionId, RskitError> {
            unimplemented!()
        }
        async fn read(&self, _id: &SessionId) -> Result<Option<SessionData>, RskitError> {
            Err(RskitError::internal("simulated DB failure"))
        }
        async fn touch(
            &self,
            _id: &SessionId,
            _ttl: std::time::Duration,
        ) -> Result<(), RskitError> {
            unimplemented!()
        }
        async fn update_data(
            &self,
            _id: &SessionId,
            _data: serde_json::Value,
        ) -> Result<(), RskitError> {
            unimplemented!()
        }
        async fn destroy(&self, _id: &SessionId) -> Result<(), RskitError> {
            unimplemented!()
        }
        async fn destroy_all_for_user(&self, _user_id: &str) -> Result<(), RskitError> {
            unimplemented!()
        }
        async fn read_by_token(
            &self,
            _token: &SessionToken,
        ) -> Result<Option<SessionData>, RskitError> {
            Err(RskitError::internal("simulated DB failure"))
        }
        async fn update_token(
            &self,
            _id: &SessionId,
            _new_token: &SessionToken,
        ) -> Result<(), RskitError> {
            unimplemented!()
        }
        async fn destroy_all_except(
            &self,
            _user_id: &str,
            _except_id: &SessionId,
        ) -> Result<(), RskitError> {
            unimplemented!()
        }
    }

    // --- Test user type + provider ---

    #[derive(Clone, Debug)]
    #[allow(dead_code)]
    struct TestUser {
        id: String,
        name: String,
    }

    struct TestUserProvider;

    impl UserProvider for TestUserProvider {
        type User = TestUser;
        async fn find_by_id(&self, id: &str) -> Result<Option<TestUser>, RskitError> {
            match id {
                "user1" => Ok(Some(TestUser {
                    id: "user1".into(),
                    name: "Alice".into(),
                })),
                _ => Ok(None),
            }
        }
    }

    // --- Test handlers ---

    async fn login_handler(mut session: SessionManager) -> &'static str {
        session.authenticate("user1").await.unwrap();
        "logged_in"
    }

    async fn check_handler(session: SessionManager) -> String {
        match session.current() {
            Some(s) => format!("user:{}", s.user_id),
            None => "no_session".to_string(),
        }
    }

    async fn auth_handler(auth: Auth<TestUser>) -> String {
        format!("hello:{}", auth.0.user.name)
    }

    async fn optional_auth_handler(auth: OptionalAuth<TestUser>) -> String {
        match auth.0 {
            Some(data) => format!("hello:{}", data.user.name),
            None => "anonymous".to_string(),
        }
    }

    // --- Test infrastructure ---

    fn build_session_router(store: Arc<dyn SessionStoreDyn>) -> (axum::Router, AppState) {
        let config = rskit::config::AppConfig {
            session_validate_fingerprint: true,
            ..Default::default()
        };
        let services =
            ServiceRegistry::new().with(UserProviderService::<TestUser>::new(TestUserProvider));
        let state = AppState {
            db: None,
            services,
            config,
            cookie_key: axum_extra::extract::cookie::Key::generate(),
            session_store: Some(store),
        };

        let router = axum::Router::new()
            .route("/login", post(login_handler))
            .route("/check", get(check_handler))
            .route("/auth", get(auth_handler))
            .route("/optional-auth", get(optional_auth_handler))
            .layer(axum::middleware::from_fn_with_state(
                state.clone(),
                rskit::middleware::session::session,
            ))
            .with_state(state.clone());

        (router, state)
    }

    /// Extract the session cookie value from a Set-Cookie response header.
    fn extract_session_cookie(
        response: &axum::http::Response<Body>,
        cookie_name: &str,
    ) -> Option<String> {
        response
            .headers()
            .get_all(header::SET_COOKIE)
            .iter()
            .find_map(|v| {
                let s = v.to_str().ok()?;
                if s.starts_with(&format!("{cookie_name}=")) {
                    Some(s.split(';').next()?.to_string())
                } else {
                    None
                }
            })
    }

    /// Check if the Set-Cookie header removes (blanks) the session cookie.
    fn cookie_is_removed(response: &axum::http::Response<Body>, cookie_name: &str) -> bool {
        response
            .headers()
            .get_all(header::SET_COOKIE)
            .iter()
            .any(|v| {
                let s = v.to_str().unwrap_or("");
                // PrivateCookieJar removal sets the cookie with empty value or Max-Age=0
                s.starts_with(&format!("{cookie_name}="))
                    && (s.contains("Max-Age=0") || s.contains("max-age=0"))
            })
    }

    const COOKIE_NAME: &str = "_rskit_session";
    const TEST_UA: &str = "Mozilla/5.0 TestBrowser";

    // --- 4.1 Middleware integration tests ---

    #[tokio::test]
    async fn session_authenticate_sets_cookie() {
        let store = Arc::new(MemoryStore::new()) as Arc<dyn SessionStoreDyn>;
        let (app, _) = build_session_router(store);

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/login")
                    .header("user-agent", TEST_UA)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let cookie = extract_session_cookie(&response, COOKIE_NAME);
        assert!(cookie.is_some(), "response should set session cookie");
    }

    #[tokio::test]
    async fn session_valid_cookie_recognized() {
        let store = Arc::new(MemoryStore::new()) as Arc<dyn SessionStoreDyn>;
        let (app, _) = build_session_router(store);

        // Login
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/login")
                    .header("user-agent", TEST_UA)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let cookie = extract_session_cookie(&response, COOKIE_NAME).unwrap();

        // Check with cookie
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/check")
                    .header("cookie", &cookie)
                    .header("user-agent", TEST_UA)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(&body[..], b"user:user1");
    }

    #[tokio::test]
    async fn session_expired_removes_stale_cookie() {
        let store = Arc::new(MemoryStore::new());
        let store_dyn = store.clone() as Arc<dyn SessionStoreDyn>;
        let (app, _) = build_session_router(store_dyn);

        // Login
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/login")
                    .header("user-agent", TEST_UA)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let cookie = extract_session_cookie(&response, COOKIE_NAME).unwrap();

        // Expire all sessions in the store
        {
            let mut sessions = store.sessions.lock().unwrap();
            for session in sessions.values_mut() {
                session.expires_at = chrono::Utc::now() - chrono::Duration::hours(1);
            }
        }

        // Request with expired cookie
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/check")
                    .header("cookie", &cookie)
                    .header("user-agent", TEST_UA)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // Handler should see no session
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(&body[..], b"no_session");
    }

    #[tokio::test]
    async fn session_fingerprint_mismatch_destroys_session() {
        let store = Arc::new(MemoryStore::new());
        let store_dyn = store.clone() as Arc<dyn SessionStoreDyn>;
        let (app, _) = build_session_router(store_dyn);

        // Login with one user-agent
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/login")
                    .header("user-agent", "BrowserA")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let cookie = extract_session_cookie(&response, COOKIE_NAME).unwrap();

        // Grab the session ID for later verification
        let session_id = {
            let sessions = store.sessions.lock().unwrap();
            sessions
                .values()
                .next()
                .expect("should have a session")
                .id
                .clone()
        };

        // Request with different user-agent (fingerprint mismatch)
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/check")
                    .header("cookie", &cookie)
                    .header("user-agent", "CompletelyDifferentBrowser")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(&body[..], b"no_session");

        // Session should be destroyed in the store
        assert!(
            store.get_session(&session_id).is_none(),
            "session should be destroyed after fingerprint mismatch"
        );
    }

    #[tokio::test]
    async fn session_transient_db_error_preserves_cookie() {
        let store = Arc::new(FailingReadStore) as Arc<dyn SessionStoreDyn>;
        // Build a router with the failing store — we can't actually login
        // because create would panic, but we can simulate having a cookie
        // from a prior session by setting the Cookie header with a value.
        // The middleware will try read_by_token, get an error, and should
        // NOT remove the cookie.
        let config = rskit::config::AppConfig::default();
        let state = AppState {
            db: None,
            services: Default::default(),
            config,
            cookie_key: axum_extra::extract::cookie::Key::generate(),
            session_store: Some(store),
        };

        // We need to create an encrypted cookie value. To do this, we'll
        // make a first request through a working store, then use that cookie
        // against the failing store with the same key.
        // Simpler approach: use PrivateCookieJar directly to encrypt a value.
        use axum_extra::extract::cookie::{Key, PrivateCookieJar};
        let key = state.cookie_key.clone();
        let jar = PrivateCookieJar::<Key>::new(key);
        let jar = jar.add(cookie::Cookie::new(COOKIE_NAME, "fake_token_value"));
        // Extract the Set-Cookie to get the encrypted value
        // Use the jar's into_response to get Set-Cookie headers
        let response: axum::http::Response<Body> = (jar, Body::empty()).into_response();
        let encrypted_cookie = response
            .headers()
            .get_all(header::SET_COOKIE)
            .iter()
            .find_map(|v| {
                let s = v.to_str().ok()?;
                if s.starts_with(&format!("{COOKIE_NAME}=")) {
                    Some(s.split(';').next()?.to_string())
                } else {
                    None
                }
            })
            .expect("should produce encrypted cookie");

        let app = axum::Router::new()
            .route("/check", get(check_handler))
            .layer(axum::middleware::from_fn_with_state(
                state.clone(),
                rskit::middleware::session::session,
            ))
            .with_state(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/check")
                    .header("cookie", &encrypted_cookie)
                    .header("user-agent", TEST_UA)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // Handler should see no session (read failed)
        let is_removed = cookie_is_removed(&response, COOKIE_NAME);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(&body[..], b"no_session");
        // Cookie should NOT be removed on transient error
        assert!(
            !is_removed,
            "cookie should NOT be removed when DB read fails"
        );
    }

    // --- 4.2 Auth extractor integration tests ---

    #[tokio::test]
    async fn auth_returns_401_without_session() {
        let store = Arc::new(MemoryStore::new()) as Arc<dyn SessionStoreDyn>;
        let (app, _) = build_session_router(store);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/auth")
                    .header("user-agent", TEST_UA)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn auth_returns_user_with_valid_session() {
        let store = Arc::new(MemoryStore::new()) as Arc<dyn SessionStoreDyn>;
        let (app, _) = build_session_router(store);

        // Login
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/login")
                    .header("user-agent", TEST_UA)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let cookie = extract_session_cookie(&response, COOKIE_NAME).unwrap();

        // Auth request
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/auth")
                    .header("cookie", &cookie)
                    .header("user-agent", TEST_UA)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(&body[..], b"hello:Alice");
    }

    #[tokio::test]
    async fn optional_auth_returns_anonymous_without_session() {
        let store = Arc::new(MemoryStore::new()) as Arc<dyn SessionStoreDyn>;
        let (app, _) = build_session_router(store);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/optional-auth")
                    .header("user-agent", TEST_UA)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(&body[..], b"anonymous");
    }
}
