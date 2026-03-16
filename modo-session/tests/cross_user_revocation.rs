//! TEST-09: Cross-user session revocation.
//!
//! Validates that an admin can revoke all sessions belonging to a specific user
//! without affecting other users, and that individual sessions can be revoked
//! by ID.

// Force the linker to include modo_session entity registration.
#[allow(unused_imports)]
use modo_session::entity::session as _;

use modo_db::sea_orm::{ConnectionTrait, Schema};
use modo_db::{DatabaseConfig, DbPool};
use modo_session::{SessionConfig, SessionMeta, SessionStore};

async fn setup_db() -> DbPool {
    let config = DatabaseConfig {
        url: "sqlite::memory:".to_string(),
        max_connections: 1,
        min_connections: 1,
        ..Default::default()
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

/// Admin revokes all sessions for user A; user A's tokens become invalid
/// while the admin's own session survives.
#[tokio::test]
async fn admin_revokes_all_user_sessions() {
    let db = setup_db().await;
    let store = SessionStore::new(&db, SessionConfig::default(), Default::default());
    let meta = test_meta();

    // User A creates 2 sessions.
    let (_sa1, ta1) = store.create(&meta, "user-a", None).await.unwrap();
    let (_sa2, ta2) = store.create(&meta, "user-a", None).await.unwrap();

    // Admin creates 1 session.
    let (_sadmin, tadmin) = store.create(&meta, "admin", None).await.unwrap();

    // Admin revokes all of user A's sessions.
    store.destroy_all_for_user("user-a").await.unwrap();

    // Both of user A's tokens are now invalid.
    assert!(
        store.read_by_token(&ta1).await.unwrap().is_none(),
        "user-a session 1 should be revoked"
    );
    assert!(
        store.read_by_token(&ta2).await.unwrap().is_none(),
        "user-a session 2 should be revoked"
    );

    // Admin's session is unaffected.
    assert!(
        store.read_by_token(&tadmin).await.unwrap().is_some(),
        "admin session should still be valid"
    );
}

/// Destroying a specific session by ID leaves other sessions for the same
/// user intact.
#[tokio::test]
async fn revoke_specific_session_by_id() {
    let db = setup_db().await;
    let store = SessionStore::new(&db, SessionConfig::default(), Default::default());
    let meta = test_meta();

    // User creates 2 sessions.
    let (s1, t1) = store.create(&meta, "user-a", None).await.unwrap();
    let (_s2, t2) = store.create(&meta, "user-a", None).await.unwrap();

    // Destroy only the first session.
    store.destroy(&s1.id).await.unwrap();

    // First session is gone.
    assert!(
        store.read_by_token(&t1).await.unwrap().is_none(),
        "first session should be destroyed"
    );

    // Second session remains.
    assert!(
        store.read_by_token(&t2).await.unwrap().is_some(),
        "second session should still be valid"
    );
}
