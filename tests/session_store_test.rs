use modo::db;
use modo::session::meta::SessionMeta;
use modo::session::{SessionConfig, Store};

async fn setup_store() -> Store {
    let config = {
        let mut c = db::SqliteConfig::default();
        c.path = ":memory:".to_string();
        c
    };
    let pool = db::connect(&config).await.unwrap();

    sqlx::query(
        "CREATE TABLE sessions (
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
        )",
    )
    .execute(&*pool)
    .await
    .unwrap();

    sqlx::query("CREATE INDEX idx_sessions_user_id ON sessions(user_id)")
        .execute(&*pool)
        .await
        .unwrap();

    sqlx::query("CREATE INDEX idx_sessions_expires_at ON sessions(expires_at)")
        .execute(&*pool)
        .await
        .unwrap();

    Store::new(&pool, SessionConfig::default())
}

fn test_meta() -> SessionMeta {
    SessionMeta::from_headers(
        "127.0.0.1".to_string(),
        "Mozilla/5.0 Chrome/120.0.0.0",
        "en-US",
        "gzip",
    )
}

#[tokio::test]
async fn test_create_and_read_by_token() {
    let store = setup_store().await;
    let meta = test_meta();

    let (session, token) = store.create(&meta, "user-1", None).await.unwrap();
    assert_eq!(session.user_id, "user-1");
    assert_eq!(session.ip_address, "127.0.0.1");
    assert!(!session.id.is_empty());

    let loaded = store.read_by_token(&token).await.unwrap();
    assert!(loaded.is_some());
    let loaded = loaded.unwrap();
    assert_eq!(loaded.id, session.id);
    assert_eq!(loaded.user_id, "user-1");
}

#[tokio::test]
async fn test_create_with_initial_data() {
    let store = setup_store().await;
    let meta = test_meta();
    let data = serde_json::json!({"cart": ["item-1"]});

    let (session, _) = store.create(&meta, "user-1", Some(data)).await.unwrap();
    assert_eq!(session.data["cart"][0], "item-1");
}

#[tokio::test]
async fn test_read_by_id() {
    let store = setup_store().await;
    let meta = test_meta();
    let (session, _) = store.create(&meta, "user-1", None).await.unwrap();

    let loaded = store.read(&session.id).await.unwrap();
    assert!(loaded.is_some());
    assert_eq!(loaded.unwrap().user_id, "user-1");
}

#[tokio::test]
async fn test_read_nonexistent_returns_none() {
    let store = setup_store().await;
    let loaded = store.read("nonexistent").await.unwrap();
    assert!(loaded.is_none());
}

#[tokio::test]
async fn test_destroy() {
    let store = setup_store().await;
    let meta = test_meta();
    let (session, token) = store.create(&meta, "user-1", None).await.unwrap();

    store.destroy(&session.id).await.unwrap();
    let loaded = store.read_by_token(&token).await.unwrap();
    assert!(loaded.is_none());
}

#[tokio::test]
async fn test_destroy_all_for_user() {
    let store = setup_store().await;
    let meta = test_meta();
    store.create(&meta, "user-1", None).await.unwrap();
    store.create(&meta, "user-1", None).await.unwrap();
    store.create(&meta, "user-2", None).await.unwrap();

    store.destroy_all_for_user("user-1").await.unwrap();

    let user1_sessions = store.list_for_user("user-1").await.unwrap();
    assert!(user1_sessions.is_empty());

    let user2_sessions = store.list_for_user("user-2").await.unwrap();
    assert_eq!(user2_sessions.len(), 1);
}

#[tokio::test]
async fn test_destroy_all_except() {
    let store = setup_store().await;
    let meta = test_meta();
    let (keep, _) = store.create(&meta, "user-1", None).await.unwrap();
    store.create(&meta, "user-1", None).await.unwrap();
    store.create(&meta, "user-1", None).await.unwrap();

    store.destroy_all_except("user-1", &keep.id).await.unwrap();

    let sessions = store.list_for_user("user-1").await.unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].id, keep.id);
}

#[tokio::test]
async fn test_rotate_token() {
    let store = setup_store().await;
    let meta = test_meta();
    let (session, old_token) = store.create(&meta, "user-1", None).await.unwrap();

    let new_token = store.rotate_token(&session.id).await.unwrap();
    assert_ne!(old_token.as_hex(), new_token.as_hex());

    // Old token should not find the session
    let old_lookup = store.read_by_token(&old_token).await.unwrap();
    assert!(old_lookup.is_none());

    // New token should find it
    let new_lookup = store.read_by_token(&new_token).await.unwrap();
    assert!(new_lookup.is_some());
    assert_eq!(new_lookup.unwrap().id, session.id);
}

#[tokio::test]
async fn test_flush_updates_data_and_timestamps() {
    let store = setup_store().await;
    let meta = test_meta();
    let (session, token) = store.create(&meta, "user-1", None).await.unwrap();

    let new_data = serde_json::json!({"theme": "dark"});
    let now = chrono::Utc::now();
    let expires = now + chrono::Duration::seconds(3600);
    store
        .flush(&session.id, &new_data, now, expires)
        .await
        .unwrap();

    let loaded = store.read_by_token(&token).await.unwrap().unwrap();
    assert_eq!(loaded.data["theme"], "dark");
}

#[tokio::test]
async fn test_touch_updates_timestamps() {
    let store = setup_store().await;
    let meta = test_meta();
    let (session, token) = store.create(&meta, "user-1", None).await.unwrap();

    let now = chrono::Utc::now();
    let new_expires = now + chrono::Duration::seconds(2_592_000 + 3600);
    store.touch(&session.id, now, new_expires).await.unwrap();

    let loaded = store.read_by_token(&token).await.unwrap().unwrap();
    assert!(loaded.expires_at > session.expires_at);
}

#[tokio::test]
async fn test_lru_eviction() {
    let config = {
        let mut c = SessionConfig::default();
        c.max_sessions_per_user = 2;
        c
    };
    let db_config = {
        let mut c = db::SqliteConfig::default();
        c.path = ":memory:".to_string();
        c
    };
    let pool = db::connect(&db_config).await.unwrap();
    sqlx::query(
        "CREATE TABLE sessions (
            id TEXT PRIMARY KEY, token_hash TEXT NOT NULL UNIQUE,
            user_id TEXT NOT NULL, ip_address TEXT NOT NULL,
            user_agent TEXT NOT NULL, device_name TEXT NOT NULL,
            device_type TEXT NOT NULL, fingerprint TEXT NOT NULL,
            data TEXT NOT NULL DEFAULT '{}', created_at TEXT NOT NULL,
            last_active_at TEXT NOT NULL, expires_at TEXT NOT NULL
        )",
    )
    .execute(&*pool)
    .await
    .unwrap();

    let store = Store::new(&pool, config);
    let meta = test_meta();

    let (s1, _) = store.create(&meta, "user-1", None).await.unwrap();
    let (_s2, _) = store.create(&meta, "user-1", None).await.unwrap();
    // Third session should evict s1 (oldest)
    let (_s3, _) = store.create(&meta, "user-1", None).await.unwrap();

    let sessions = store.list_for_user("user-1").await.unwrap();
    assert_eq!(sessions.len(), 2);
    assert!(sessions.iter().all(|s| s.id != s1.id));
}

#[tokio::test]
async fn test_cleanup_expired() {
    let store = setup_store().await;
    let meta = test_meta();

    // Create a session — it has a 30-day TTL so it's not expired
    let (_session, _) = store.create(&meta, "user-1", None).await.unwrap();
    let count = store.cleanup_expired().await.unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn test_list_for_user_ordered_by_last_active() {
    let store = setup_store().await;
    let meta = test_meta();

    let (s1, _) = store.create(&meta, "user-1", None).await.unwrap();
    let (s2, _) = store.create(&meta, "user-1", None).await.unwrap();

    // Touch s1 to make it more recent
    let now = chrono::Utc::now();
    let expires = now + chrono::Duration::seconds(3600);
    store.touch(&s1.id, now, expires).await.unwrap();

    let sessions = store.list_for_user("user-1").await.unwrap();
    assert_eq!(sessions.len(), 2);
    assert_eq!(sessions[0].id, s1.id); // s1 is most recent
    assert_eq!(sessions[1].id, s2.id);
}

#[tokio::test]
async fn test_cleanup_expired_deletes_rows() {
    let config = {
        let mut c = db::SqliteConfig::default();
        c.path = ":memory:".to_string();
        c
    };
    let pool = db::connect(&config).await.unwrap();
    sqlx::query(
        "CREATE TABLE sessions (
            id TEXT PRIMARY KEY, token_hash TEXT NOT NULL UNIQUE,
            user_id TEXT NOT NULL, ip_address TEXT NOT NULL,
            user_agent TEXT NOT NULL, device_name TEXT NOT NULL,
            device_type TEXT NOT NULL, fingerprint TEXT NOT NULL,
            data TEXT NOT NULL DEFAULT '{}', created_at TEXT NOT NULL,
            last_active_at TEXT NOT NULL, expires_at TEXT NOT NULL
        )",
    )
    .execute(&*pool)
    .await
    .unwrap();
    sqlx::query("CREATE INDEX idx_sessions_user_id ON sessions(user_id)")
        .execute(&*pool)
        .await
        .unwrap();
    sqlx::query("CREATE INDEX idx_sessions_expires_at ON sessions(expires_at)")
        .execute(&*pool)
        .await
        .unwrap();

    let store = Store::new(&pool, SessionConfig::default());
    let meta = test_meta();
    let (session, _) = store.create(&meta, "user-1", None).await.unwrap();

    // Manually expire the session
    sqlx::query("UPDATE sessions SET expires_at = ? WHERE id = ?")
        .bind("2020-01-01T00:00:00+00:00")
        .bind(&session.id)
        .execute(&*pool)
        .await
        .unwrap();

    let count = store.cleanup_expired().await.unwrap();
    assert_eq!(count, 1);

    let loaded = store.read(&session.id).await.unwrap();
    assert!(loaded.is_none());
}

#[tokio::test]
async fn test_max_sessions_per_user_one() {
    let config = {
        let mut c = db::SqliteConfig::default();
        c.path = ":memory:".to_string();
        c
    };
    let pool = db::connect(&config).await.unwrap();
    sqlx::query(
        "CREATE TABLE sessions (
            id TEXT PRIMARY KEY, token_hash TEXT NOT NULL UNIQUE,
            user_id TEXT NOT NULL, ip_address TEXT NOT NULL,
            user_agent TEXT NOT NULL, device_name TEXT NOT NULL,
            device_type TEXT NOT NULL, fingerprint TEXT NOT NULL,
            data TEXT NOT NULL DEFAULT '{}', created_at TEXT NOT NULL,
            last_active_at TEXT NOT NULL, expires_at TEXT NOT NULL
        )",
    )
    .execute(&*pool)
    .await
    .unwrap();
    sqlx::query("CREATE INDEX idx_sessions_user_id ON sessions(user_id)")
        .execute(&*pool)
        .await
        .unwrap();
    sqlx::query("CREATE INDEX idx_sessions_expires_at ON sessions(expires_at)")
        .execute(&*pool)
        .await
        .unwrap();

    let session_config = {
        let mut c = SessionConfig::default();
        c.max_sessions_per_user = 1;
        c
    };
    let store = Store::new(&pool, session_config);
    let meta = test_meta();

    let (_s1, _) = store.create(&meta, "user-1", None).await.unwrap();
    let (s2, _) = store.create(&meta, "user-1", None).await.unwrap();

    let sessions = store.list_for_user("user-1").await.unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].id, s2.id); // newest survives
}

#[tokio::test]
async fn test_list_for_user_excludes_expired() {
    let config = {
        let mut c = db::SqliteConfig::default();
        c.path = ":memory:".to_string();
        c
    };
    let pool = db::connect(&config).await.unwrap();
    sqlx::query(
        "CREATE TABLE sessions (
            id TEXT PRIMARY KEY, token_hash TEXT NOT NULL UNIQUE,
            user_id TEXT NOT NULL, ip_address TEXT NOT NULL,
            user_agent TEXT NOT NULL, device_name TEXT NOT NULL,
            device_type TEXT NOT NULL, fingerprint TEXT NOT NULL,
            data TEXT NOT NULL DEFAULT '{}', created_at TEXT NOT NULL,
            last_active_at TEXT NOT NULL, expires_at TEXT NOT NULL
        )",
    )
    .execute(&*pool)
    .await
    .unwrap();
    sqlx::query("CREATE INDEX idx_sessions_user_id ON sessions(user_id)")
        .execute(&*pool)
        .await
        .unwrap();
    sqlx::query("CREATE INDEX idx_sessions_expires_at ON sessions(expires_at)")
        .execute(&*pool)
        .await
        .unwrap();

    let store = Store::new(&pool, SessionConfig::default());
    let meta = test_meta();
    let (session, _) = store.create(&meta, "user-1", None).await.unwrap();

    // Manually expire the session
    sqlx::query("UPDATE sessions SET expires_at = ? WHERE id = ?")
        .bind("2020-01-01T00:00:00+00:00")
        .bind(&session.id)
        .execute(&*pool)
        .await
        .unwrap();

    let sessions = store.list_for_user("user-1").await.unwrap();
    assert!(sessions.is_empty());
}

#[tokio::test]
async fn test_read_by_token_returns_none_for_expired() {
    let config = {
        let mut c = db::SqliteConfig::default();
        c.path = ":memory:".to_string();
        c
    };
    let pool = db::connect(&config).await.unwrap();
    sqlx::query(
        "CREATE TABLE sessions (
            id TEXT PRIMARY KEY, token_hash TEXT NOT NULL UNIQUE,
            user_id TEXT NOT NULL, ip_address TEXT NOT NULL,
            user_agent TEXT NOT NULL, device_name TEXT NOT NULL,
            device_type TEXT NOT NULL, fingerprint TEXT NOT NULL,
            data TEXT NOT NULL DEFAULT '{}', created_at TEXT NOT NULL,
            last_active_at TEXT NOT NULL, expires_at TEXT NOT NULL
        )",
    )
    .execute(&*pool)
    .await
    .unwrap();
    sqlx::query("CREATE INDEX idx_sessions_user_id ON sessions(user_id)")
        .execute(&*pool)
        .await
        .unwrap();
    sqlx::query("CREATE INDEX idx_sessions_expires_at ON sessions(expires_at)")
        .execute(&*pool)
        .await
        .unwrap();

    let store = Store::new(&pool, SessionConfig::default());
    let meta = test_meta();
    let (session, token) = store.create(&meta, "user-1", None).await.unwrap();

    // Manually expire the session
    sqlx::query("UPDATE sessions SET expires_at = ? WHERE id = ?")
        .bind("2020-01-01T00:00:00+00:00")
        .bind(&session.id)
        .execute(&*pool)
        .await
        .unwrap();

    let loaded = store.read_by_token(&token).await.unwrap();
    assert!(loaded.is_none());
}
