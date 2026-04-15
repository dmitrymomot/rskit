#![cfg(feature = "test-helpers")]

use modo::auth::session::cookie::{CookieSessionService, CookieSessionsConfig};
use modo::testing::{TestDb, TestSession};

#[tokio::test]
async fn service_constructs_and_cleanup_expired_returns_zero() {
    let db = TestDb::new().await;
    // Create the authenticated_sessions schema via TestSession's side effect.
    let _ts = TestSession::new(&db).await;
    let mut cfg = CookieSessionsConfig::default();
    cfg.cookie.secret = "a".repeat(64);
    let svc = CookieSessionService::new(db.db(), cfg).expect("service construction failed");
    let removed = svc.cleanup_expired().await.unwrap();
    assert_eq!(removed, 0);
}

#[tokio::test]
async fn service_list_returns_empty_for_unknown_user() {
    use modo::db::ConnExt;
    let db = TestDb::new().await;
    db.db()
        .conn()
        .execute_raw(modo::testing::TestSession::SCHEMA_SQL, ())
        .await
        .unwrap();
    let mut cfg = CookieSessionsConfig::default();
    cfg.cookie.secret = "a".repeat(64);
    let svc = CookieSessionService::new(db.db(), cfg).unwrap();
    let rows = svc.list("nobody").await.unwrap();
    assert_eq!(rows.len(), 0);
}
