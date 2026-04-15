use cookie::{Cookie, CookieJar};

use crate::auth::session::SessionConfig;
use crate::auth::session::meta::SessionMeta;
use crate::auth::session::store::SessionStore;
use crate::cookie::{CookieConfig, Key, key_from_config};

use super::db::TestDb;

const SESSIONS_TABLE_SQL: &str = "CREATE TABLE authenticated_sessions (
        id TEXT PRIMARY KEY,
        session_token_hash TEXT NOT NULL UNIQUE,
        user_id TEXT NOT NULL,
        ip_address TEXT NOT NULL,
        user_agent TEXT NOT NULL,
        device_name TEXT NOT NULL,
        device_type TEXT NOT NULL,
        fingerprint TEXT NOT NULL,
        data TEXT NOT NULL DEFAULT '{}',
        created_at TEXT NOT NULL,
        last_active_at TEXT NOT NULL,
        expires_at TEXT NOT NULL
    )";

#[allow(dead_code)]
const SESSIONS_INDEXES_SQL: &[&str] = &[
    "CREATE INDEX idx_sessions_user_id ON authenticated_sessions (user_id)",
    "CREATE INDEX idx_sessions_expires_at ON authenticated_sessions (expires_at)",
];

/// Session infrastructure for integration tests.
///
/// `TestSession` sets up an in-memory `sessions` table on the provided
/// [`TestDb`], derives a signing key, and exposes helpers for authenticating
/// test users and building the [`SessionLayer`](crate::auth::session::SessionLayer)
/// needed by [`super::TestApp`].
///
/// # Example
///
/// ```rust,no_run
/// # #[cfg(feature = "test-helpers")]
/// # async fn example() {
/// use axum::routing::get;
/// use modo::auth::session::Session;
/// use modo::testing::{TestApp, TestDb, TestSession};
///
/// async fn whoami(session: Session) -> String {
///     session.user_id().unwrap_or_else(|| "anonymous".to_string())
/// }
///
/// let db = TestDb::new().await;
/// let session = TestSession::new(&db).await;
///
/// let app = TestApp::builder()
///     .route("/me", get(whoami))
///     .layer(session.layer())
///     .build();
///
/// let cookie = session.authenticate("user-1").await;
/// let res = app.get("/me").header("cookie", &cookie).send().await;
/// assert_eq!(res.text(), "user-1");
/// # }
/// ```
pub struct TestSession {
    store: SessionStore,
    cookie_config: CookieConfig,
    key: Key,
    session_config: SessionConfig,
}

impl TestSession {
    /// Create a `TestSession` with default [`SessionConfig`] and a
    /// test-suitable [`CookieConfig`] (insecure, lax same-site, 64-char secret).
    ///
    /// Creates the `authenticated_sessions` table and indexes on `db`.
    ///
    /// # Panics
    ///
    /// Panics if the sessions table cannot be created or the cookie key
    /// cannot be derived.
    pub async fn new(db: &TestDb) -> Self {
        let cookie_config = CookieConfig {
            secret: "a".repeat(64),
            secure: false,
            http_only: true,
            same_site: "lax".to_string(),
        };
        Self::with_config(db, SessionConfig::default(), cookie_config).await
    }

    /// Create a `TestSession` with explicit [`SessionConfig`] and [`CookieConfig`].
    ///
    /// Creates the `authenticated_sessions` table and indexes on `db`.
    ///
    /// # Panics
    ///
    /// Panics if the sessions table cannot be created or the cookie key
    /// cannot be derived.
    pub async fn with_config(
        db: &TestDb,
        session_config: SessionConfig,
        cookie_config: CookieConfig,
    ) -> Self {
        use crate::db::ConnExt;
        db.db()
            .conn()
            .execute_raw(SESSIONS_TABLE_SQL, ())
            .await
            .expect("failed to create sessions table");
        for sql in SESSIONS_INDEXES_SQL {
            db.db()
                .conn()
                .execute_raw(sql, ())
                .await
                .expect("failed to create sessions index");
        }

        let key = key_from_config(&cookie_config).expect("failed to derive cookie key");
        let store = SessionStore::new(db.db(), session_config.clone());

        Self {
            store,
            cookie_config,
            key,
            session_config,
        }
    }

    /// Create a session for `user_id` with empty session data and return the
    /// signed cookie string (e.g. `"_session=<signed-value>"`).
    ///
    /// Pass the returned value as the `cookie` header in subsequent requests.
    ///
    /// # Panics
    ///
    /// Panics if the session cannot be created in the store.
    pub async fn authenticate(&self, user_id: &str) -> String {
        self.authenticate_with(user_id, serde_json::json!({})).await
    }

    /// Create a session for `user_id` with custom JSON `data` and return the
    /// signed cookie string.
    ///
    /// Pass the returned value as the `cookie` header in subsequent requests.
    ///
    /// # Panics
    ///
    /// Panics if the session cannot be created in the store.
    pub async fn authenticate_with(&self, user_id: &str, data: serde_json::Value) -> String {
        let meta = SessionMeta::from_headers("127.0.0.1".to_string(), "", "", "");

        let (_session_data, token): (crate::auth::session::store::SessionData, _) = self
            .store
            .create(&meta, user_id, Some(data))
            .await
            .expect("failed to create test session");

        let cookie_name = &self.session_config.cookie_name;
        let mut jar = CookieJar::new();
        jar.signed_mut(&self.key)
            .add(Cookie::new(cookie_name.to_string(), token.as_hex()));
        let signed_value = jar
            .get(cookie_name)
            .expect("cookie was just added")
            .value()
            .to_string();

        format!("{cookie_name}={signed_value}")
    }

    /// Build a [`SessionLayer`](crate::auth::session::SessionLayer) configured with
    /// the same store and cookie settings as this `TestSession`.
    ///
    /// Apply this layer to a [`super::TestAppBuilder`] so that handlers can
    /// use the [`Session`](crate::auth::session::Session) extractor.
    pub fn layer(&self) -> crate::auth::session::SessionLayer {
        crate::auth::session::layer(self.store.clone(), &self.cookie_config, &self.key)
    }
}
