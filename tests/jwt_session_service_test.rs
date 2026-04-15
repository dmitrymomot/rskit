#![cfg(feature = "test-helpers")]

use modo::auth::session::jwt::{JwtSessionService, JwtSessionsConfig};
use modo::auth::session::meta::SessionMeta;
use modo::db::ConnExt;
use modo::testing::{TestDb, TestSession};

fn meta() -> SessionMeta {
    SessionMeta::from_headers("127.0.0.1".to_string(), "test/1.0", "", "")
}

async fn setup() -> (TestDb, JwtSessionService) {
    let db = TestDb::new().await;
    db.db()
        .conn()
        .execute_raw(TestSession::SCHEMA_SQL, ())
        .await
        .unwrap();
    let mut config = JwtSessionsConfig::default();
    config.signing_secret = "test-secret-32-bytes-long-okay-??".into();
    let svc = JwtSessionService::new(db.db(), config).unwrap();
    (db, svc)
}

#[tokio::test]
async fn authenticate_returns_token_pair_and_creates_row() {
    let (_db, svc) = setup().await;
    let pair = svc.authenticate("user_1", &meta()).await.unwrap();
    assert!(!pair.access_token.is_empty());
    assert!(!pair.refresh_token.is_empty());
    assert_ne!(pair.access_token, pair.refresh_token);

    let rows = svc.list("user_1").await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].user_id, "user_1");
}

#[tokio::test]
async fn rotate_invalidates_old_refresh_and_issues_new_pair() {
    let (_db, svc) = setup().await;
    let original = svc.authenticate("user_1", &meta()).await.unwrap();
    let new_pair = svc.rotate(&original.refresh_token).await.unwrap();
    assert_ne!(new_pair.refresh_token, original.refresh_token);

    let err = svc.rotate(&original.refresh_token).await.unwrap_err();
    assert_eq!(err.error_code(), Some("auth:session_not_found"));
}

#[tokio::test]
async fn logout_revokes_session() {
    let (_db, svc) = setup().await;
    let pair = svc.authenticate("user_1", &meta()).await.unwrap();
    svc.logout(&pair.access_token).await.unwrap();

    let err = svc.rotate(&pair.refresh_token).await.unwrap_err();
    assert_eq!(err.error_code(), Some("auth:session_not_found"));
}

#[tokio::test]
async fn rotate_rejects_access_token_with_aud_mismatch() {
    let (_db, svc) = setup().await;
    let pair = svc.authenticate("user_1", &meta()).await.unwrap();
    let err = svc.rotate(&pair.access_token).await.unwrap_err();
    assert_eq!(err.error_code(), Some("auth:aud_mismatch"));
}
