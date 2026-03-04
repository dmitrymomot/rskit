use rskit::session::{SessionMeta, SessionStore, SqliteSessionStore};
use sea_orm::{ConnectionTrait, Database};
use std::sync::Arc;
use std::time::Duration;

async fn setup_store() -> Arc<SqliteSessionStore> {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    // Run SQLite pragmas
    db.execute_unprepared("PRAGMA journal_mode=WAL")
        .await
        .unwrap();

    let store = SqliteSessionStore::new(db, Duration::from_secs(3600), 5);
    store.initialize().await.unwrap();
    Arc::new(store)
}

fn test_meta() -> SessionMeta {
    SessionMeta {
        ip_address: "127.0.0.1".to_string(),
        user_agent: "TestAgent/1.0".to_string(),
        device_name: "Test on Test".to_string(),
        device_type: "desktop".to_string(),
        fingerprint: "testfingerprint".to_string(),
    }
}

#[tokio::test]
async fn session_create_read_destroy() {
    let store = setup_store().await;
    let meta = test_meta();

    // Create
    let id = store.create("user123", &meta).await.unwrap();

    // Read
    let session = store.read(&id).await.unwrap().unwrap();
    assert_eq!(session.user_id, "user123");
    assert_eq!(session.ip_address, "127.0.0.1");
    assert_eq!(session.device_type, "desktop");

    // Destroy
    store.destroy(&id).await.unwrap();
    assert!(store.read(&id).await.unwrap().is_none());
}

#[tokio::test]
async fn session_with_custom_data() {
    let store = setup_store().await;
    let meta = test_meta();

    let id = store
        .create_with("user123", &meta, serde_json::json!({"theme": "dark"}))
        .await
        .unwrap();

    let session = store.read(&id).await.unwrap().unwrap();
    assert_eq!(session.data["theme"], "dark");
}

#[tokio::test]
async fn session_max_per_user_eviction() {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    let store = SqliteSessionStore::new(
        db,
        Duration::from_secs(3600),
        2, // max 2 sessions
    );
    store.initialize().await.unwrap();

    let meta = test_meta();
    let id1 = store.create("user1", &meta).await.unwrap();
    let _id2 = store.create("user1", &meta).await.unwrap();
    let _id3 = store.create("user1", &meta).await.unwrap(); // evicts id1

    assert!(store.read(&id1).await.unwrap().is_none());
}
