//! TEST-12: Concurrent session creation stress test.

// Force the linker to include modo_session entity registration.
#[allow(unused_imports)]
use modo_session::entity::session as _;

mod common;

use common::setup_db;
use modo_session::{SessionConfig, SessionMeta, SessionStore};
use std::collections::HashSet;
use std::sync::Arc;
use tokio::task::JoinSet;

fn test_meta(user_num: usize) -> SessionMeta {
    SessionMeta::from_headers(
        format!("127.0.0.{user_num}"),
        "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) Chrome/120.0.0.0",
        "en-US",
        "gzip",
    )
}

#[tokio::test]
async fn concurrent_session_creation() {
    let db = setup_db().await;
    let config = SessionConfig {
        max_sessions_per_user: 100, // High limit for stress test
        ..Default::default()
    };
    let store = Arc::new(SessionStore::new(&db, config, Default::default()));

    let mut set = JoinSet::new();

    // 5 users x 10 sessions each = 50 sessions
    for user_num in 0..5_usize {
        for session_num in 0..10_usize {
            let store = store.clone();
            let meta = test_meta(user_num);
            let user_id = format!("user_{user_num}");
            set.spawn(async move {
                let (session, token) =
                    store
                        .create(&meta, &user_id, None)
                        .await
                        .unwrap_or_else(|e| {
                            panic!("Failed to create session for {user_id} #{session_num}: {e}")
                        });

                // Read back by token to verify it was persisted correctly
                let found = store.read_by_token(&token).await.unwrap_or_else(|e| {
                    panic!("Failed to read session by token for {user_id}: {e}")
                });
                assert!(
                    found.is_some(),
                    "Session for {user_id} #{session_num} should be found by token"
                );
                let found = found.unwrap();
                assert_eq!(found.id, session.id);
                assert_eq!(found.user_id, user_id);

                session.id.to_string()
            });
        }
    }

    let mut all_ids: Vec<String> = Vec::new();
    while let Some(result) = set.join_next().await {
        let session_id = result.expect("Task panicked");
        all_ids.push(session_id);
    }

    // Verify total count
    assert_eq!(
        all_ids.len(),
        50,
        "Expected 50 sessions, got {}",
        all_ids.len()
    );

    // Verify all session IDs are unique
    let unique_ids: HashSet<&String> = all_ids.iter().collect();
    assert_eq!(
        unique_ids.len(),
        50,
        "Expected 50 unique session IDs, got {}",
        unique_ids.len()
    );
}
