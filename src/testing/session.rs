use cookie::{Cookie, CookieJar};

use crate::cookie::{CookieConfig, Key, key_from_config};
use crate::session::meta::SessionMeta;
use crate::session::{SessionConfig, Store};

use super::db::TestDb;

const SESSIONS_TABLE_SQL: &str = "CREATE TABLE modo_sessions (
        id TEXT PRIMARY KEY,
        token_hash TEXT NOT NULL UNIQUE,
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

pub struct TestSession {
    store: Store,
    cookie_config: CookieConfig,
    key: Key,
    session_config: SessionConfig,
}

impl TestSession {
    pub async fn new(db: &TestDb) -> Self {
        let cookie_config = CookieConfig {
            secret: "a".repeat(64),
            secure: false,
            http_only: true,
            same_site: "lax".to_string(),
        };
        Self::with_config(db, SessionConfig::default(), cookie_config).await
    }

    pub async fn with_config(
        db: &TestDb,
        session_config: SessionConfig,
        cookie_config: CookieConfig,
    ) -> Self {
        sqlx::query(SESSIONS_TABLE_SQL)
            .execute(&*db.pool())
            .await
            .expect("failed to create modo_sessions table");

        let key = key_from_config(&cookie_config).expect("failed to derive cookie key");
        let store = Store::new(&db.pool(), session_config.clone());

        Self {
            store,
            cookie_config,
            key,
            session_config,
        }
    }

    pub async fn authenticate(&self, user_id: &str) -> String {
        self.authenticate_with(user_id, serde_json::json!({})).await
    }

    pub async fn authenticate_with(&self, user_id: &str, data: serde_json::Value) -> String {
        let meta = SessionMeta::from_headers("127.0.0.1".to_string(), "", "", "");

        let (_session_data, token) = self
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

    pub fn layer(&self) -> crate::session::SessionLayer {
        crate::session::layer(self.store.clone(), &self.cookie_config, &self.key)
    }
}
