use modo_db::sea_orm::{ConnectionTrait, Schema};
use modo_db::{DatabaseConfig, DbPool};
use modo_session::{SessionConfig, SessionMeta, SessionStore};

async fn setup_db() -> DbPool {
    let config = DatabaseConfig {
        url: "sqlite::memory:".to_string(),
        max_connections: 1,
        min_connections: 1,
    };
    let db = modo_db::connect(&config).await.expect("Failed to connect");

    let schema = Schema::new(db.connection().get_database_backend());
    let mut builder = schema.builder();
    let reg = modo_db::inventory::iter::<modo_db::EntityRegistration>()
        .find(|r| r.table_name == "modo_sessions")
        .unwrap();
    builder = (reg.register_fn)(builder);
    builder
        .sync(db.connection())
        .await
        .expect("Schema sync failed");
    for sql in reg.extra_sql {
        db.connection()
            .execute_unprepared(sql)
            .await
            .expect("Extra SQL failed");
    }
    db
}

fn test_meta() -> SessionMeta {
    SessionMeta::from_headers(
        "127.0.0.1".to_string(),
        "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) Chrome/120.0.0.0",
        "en-US",
        "gzip",
    )
}

fn test_config() -> SessionConfig {
    SessionConfig::default()
}

#[tokio::test]
async fn create_and_read_by_token() {
    let db = setup_db().await;
    let store = SessionStore::new(&db, test_config(), Default::default());
    let meta = test_meta();

    let (session, token) = store.create(&meta, "user1", None).await.unwrap();
    assert_eq!(session.user_id, "user1");
    let found = store.read_by_token(&token).await.unwrap();
    assert!(found.is_some());
    let found = found.unwrap();
    assert_eq!(found.id, session.id);
    assert_eq!(found.user_id, "user1");
}

#[tokio::test]
async fn destroy_removes_session() {
    let db = setup_db().await;
    let store = SessionStore::new(&db, test_config(), Default::default());
    let meta = test_meta();

    let (session, token) = store.create(&meta, "user1", None).await.unwrap();
    store.destroy(&session.id).await.unwrap();

    let found = store.read_by_token(&token).await.unwrap();
    assert!(found.is_none());
}

#[tokio::test]
async fn rotate_token_changes_hash() {
    let db = setup_db().await;
    let store = SessionStore::new(&db, test_config(), Default::default());
    let meta = test_meta();

    let (session, old_token) = store.create(&meta, "user1", None).await.unwrap();
    let new_token = store.rotate_token(&session.id).await.unwrap();

    assert!(store.read_by_token(&old_token).await.unwrap().is_none());
    let found = store.read_by_token(&new_token).await.unwrap();
    assert!(found.is_some());
    assert_eq!(found.unwrap().id, session.id);
}

#[tokio::test]
async fn max_sessions_evicts_oldest() {
    let db = setup_db().await;
    let mut config = test_config();
    config.max_sessions_per_user = 2;
    let store = SessionStore::new(&db, config, Default::default());
    let meta = test_meta();

    let (s1, t1) = store.create(&meta, "user1", None).await.unwrap();
    let (_s2, t2) = store.create(&meta, "user1", None).await.unwrap();
    let (_s3, t3) = store.create(&meta, "user1", None).await.unwrap();

    // First session should be evicted (FIFO)
    assert!(store.read(&s1.id).await.unwrap().is_none());
    assert!(store.read_by_token(&t1).await.unwrap().is_none());
    // Second and third should survive
    assert!(store.read_by_token(&t2).await.unwrap().is_some());
    assert!(store.read_by_token(&t3).await.unwrap().is_some());
}

#[tokio::test]
async fn list_for_user_returns_all_active() {
    let db = setup_db().await;
    let store = SessionStore::new(&db, test_config(), Default::default());
    let meta = test_meta();

    store.create(&meta, "user1", None).await.unwrap();
    store.create(&meta, "user1", None).await.unwrap();
    store.create(&meta, "user2", None).await.unwrap();

    let user1_sessions = store.list_for_user("user1").await.unwrap();
    assert_eq!(user1_sessions.len(), 2);

    let user2_sessions = store.list_for_user("user2").await.unwrap();
    assert_eq!(user2_sessions.len(), 1);
}

#[tokio::test]
async fn destroy_all_except_keeps_one() {
    let db = setup_db().await;
    let store = SessionStore::new(&db, test_config(), Default::default());
    let meta = test_meta();

    let (s1, _) = store.create(&meta, "user1", None).await.unwrap();
    let (_s2, t2) = store.create(&meta, "user1", None).await.unwrap();
    let (_s3, t3) = store.create(&meta, "user1", None).await.unwrap();

    store.destroy_all_except("user1", &s1.id).await.unwrap();

    assert!(store.read(&s1.id).await.unwrap().is_some());
    assert!(store.read_by_token(&t2).await.unwrap().is_none());
    assert!(store.read_by_token(&t3).await.unwrap().is_none());
}

#[tokio::test]
async fn update_data_roundtrip() {
    let db = setup_db().await;
    let store = SessionStore::new(&db, test_config(), Default::default());
    let meta = test_meta();

    let (session, _) = store.create(&meta, "user1", None).await.unwrap();

    let data = serde_json::json!({"theme": "dark", "lang": "en"});
    store.update_data(&session.id, data.clone()).await.unwrap();

    let found = store.read(&session.id).await.unwrap().unwrap();
    assert_eq!(found.data, data);
}

#[tokio::test]
async fn cleanup_expired_removes_old() {
    let db = setup_db().await;
    let mut config = test_config();
    config.session_ttl_secs = 0;
    let store = SessionStore::new(&db, config, Default::default());
    let meta = test_meta();

    store.create(&meta, "user1", None).await.unwrap();
    store.create(&meta, "user1", None).await.unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    let count = store.cleanup_expired().await.unwrap();
    assert_eq!(count, 2);

    let sessions = store.list_for_user("user1").await.unwrap();
    assert!(sessions.is_empty());
}
