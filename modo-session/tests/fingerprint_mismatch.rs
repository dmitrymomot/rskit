//! TEST-08: Fingerprint mismatch detection.
//!
//! Validates that session fingerprint comparison works correctly:
//! - Mismatched fingerprints (different User-Agent) are detected and the session
//!   is destroyed (simulating middleware behaviour when `validate_fingerprint` is true).
//! - Matching fingerprints leave the session intact.

// Force the linker to include modo_session entity registration.
#[allow(unused_imports)]
use modo_session::entity::session as _;

mod common;

use common::setup_db;
use modo_session::{SessionConfig, SessionMeta, SessionStore};

fn chrome_meta() -> SessionMeta {
    SessionMeta::from_headers(
        "127.0.0.1".to_string(),
        "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) Chrome/120.0.0.0",
        "en-US",
        "gzip",
    )
}

fn firefox_meta() -> SessionMeta {
    SessionMeta::from_headers(
        "127.0.0.1".to_string(),
        "Mozilla/5.0 (X11; Linux x86_64; rv:121.0) Gecko/20100101 Firefox/121.0",
        "en-US",
        "gzip",
    )
}

/// When a session is replayed with a different User-Agent the fingerprints
/// differ. The middleware would destroy such a session; here we simulate
/// that flow at the store level.
#[tokio::test]
async fn fingerprint_mismatch_destroys_session() {
    let db = setup_db().await;
    let store = SessionStore::new(&db, SessionConfig::default(), Default::default());

    // Create a session with Chrome UA.
    let chrome = chrome_meta();
    let (session, token) = store.create(&chrome, "user-a", None).await.unwrap();

    // Session exists.
    let found = store.read_by_token(&token).await.unwrap();
    assert!(found.is_some(), "session should exist after creation");

    // Simulate a replayed request with Firefox UA.
    let firefox = firefox_meta();

    // Fingerprints must differ.
    assert_ne!(
        session.fingerprint, firefox.fingerprint,
        "Chrome and Firefox fingerprints should differ"
    );

    // Middleware would destroy the session on mismatch; simulate that.
    store.destroy(&session.id).await.unwrap();

    // Session is gone.
    let found = store.read_by_token(&token).await.unwrap();
    assert!(
        found.is_none(),
        "session should be destroyed after fingerprint mismatch"
    );
}

/// When a session is loaded back with the same headers, fingerprints match
/// and the session remains accessible.
#[tokio::test]
async fn fingerprint_match_preserves_session() {
    let db = setup_db().await;
    let store = SessionStore::new(&db, SessionConfig::default(), Default::default());

    let meta = chrome_meta();
    let (session, token) = store.create(&meta, "user-a", None).await.unwrap();

    // Re-derive fingerprint from the same headers.
    let same_meta = chrome_meta();
    assert_eq!(
        session.fingerprint, same_meta.fingerprint,
        "identical headers should produce the same fingerprint"
    );

    // Session is still accessible.
    let found = store.read_by_token(&token).await.unwrap();
    assert!(
        found.is_some(),
        "session should survive when fingerprint matches"
    );
    assert_eq!(found.unwrap().id, session.id);
}
