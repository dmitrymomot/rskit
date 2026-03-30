#![cfg(feature = "apikey")]

use modo::apikey::{ApiKeyConfig, ApiKeyStore, CreateKeyRequest};
use modo::testing::TestDb;

const SCHEMA: &str = "\
CREATE TABLE api_keys (
    id            TEXT PRIMARY KEY,
    key_hash      TEXT NOT NULL,
    tenant_id     TEXT NOT NULL,
    name          TEXT NOT NULL,
    scopes        TEXT NOT NULL DEFAULT '[]',
    expires_at    TEXT,
    last_used_at  TEXT,
    created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    revoked_at    TEXT
);
CREATE INDEX idx_api_keys_tenant ON api_keys(tenant_id);
CREATE INDEX idx_api_keys_created ON api_keys(created_at);
";

async fn test_store() -> ApiKeyStore {
    let db = TestDb::new().await.exec(SCHEMA).await;
    ApiKeyStore::new(db.db(), ApiKeyConfig::default()).unwrap()
}

fn test_request(tenant_id: &str) -> CreateKeyRequest {
    CreateKeyRequest {
        tenant_id: tenant_id.into(),
        name: "Test key".into(),
        scopes: vec!["read:orders".into()],
        expires_at: None,
    }
}

#[tokio::test]
async fn create_returns_raw_token_with_correct_prefix() {
    let store = test_store().await;
    let created = store.create(&test_request("t1")).await.unwrap();

    assert!(created.raw_token.starts_with("modo_"));
    assert_eq!(created.name, "Test key");
    assert_eq!(created.tenant_id, "t1");
    assert_eq!(created.scopes, vec!["read:orders"]);
    assert!(!created.id.is_empty());
    assert!(!created.created_at.is_empty());
}

#[tokio::test]
async fn verify_valid_token() {
    let store = test_store().await;
    let created = store.create(&test_request("t1")).await.unwrap();

    let meta = store.verify(&created.raw_token).await.unwrap();
    assert_eq!(meta.id, created.id);
    assert_eq!(meta.tenant_id, "t1");
    assert_eq!(meta.name, "Test key");
    assert_eq!(meta.scopes, vec!["read:orders"]);
}

#[tokio::test]
async fn verify_wrong_token_returns_unauthorized() {
    let store = test_store().await;
    store.create(&test_request("t1")).await.unwrap();

    let err = store
        .verify("modo_00000000000000000000000000wrong")
        .await
        .unwrap_err();
    assert_eq!(err.status(), http::StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn verify_malformed_token_returns_unauthorized() {
    let store = test_store().await;

    let err = store.verify("not-a-token").await.unwrap_err();
    assert_eq!(err.status(), http::StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn verify_wrong_prefix_returns_unauthorized() {
    let store = test_store().await;
    let created = store.create(&test_request("t1")).await.unwrap();
    let wrong_prefix = created.raw_token.replacen("modo_", "sk_", 1);

    let err = store.verify(&wrong_prefix).await.unwrap_err();
    assert_eq!(err.status(), http::StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn revoke_then_verify_returns_unauthorized() {
    let store = test_store().await;
    let created = store.create(&test_request("t1")).await.unwrap();

    store.revoke(&created.id).await.unwrap();

    let err = store.verify(&created.raw_token).await.unwrap_err();
    assert_eq!(err.status(), http::StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn revoke_nonexistent_returns_not_found() {
    let store = test_store().await;

    let err = store.revoke("nonexistent").await.unwrap_err();
    assert_eq!(err.status(), http::StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn list_returns_keys_for_tenant() {
    let store = test_store().await;
    store.create(&test_request("t1")).await.unwrap();
    store.create(&test_request("t1")).await.unwrap();
    store.create(&test_request("t2")).await.unwrap();

    let t1_keys = store.list("t1").await.unwrap();
    assert_eq!(t1_keys.len(), 2);

    let t2_keys = store.list("t2").await.unwrap();
    assert_eq!(t2_keys.len(), 1);

    let t3_keys = store.list("t3").await.unwrap();
    assert_eq!(t3_keys.len(), 0);
}

#[tokio::test]
async fn list_excludes_revoked_keys() {
    let store = test_store().await;
    let key1 = store.create(&test_request("t1")).await.unwrap();
    store.create(&test_request("t1")).await.unwrap();

    store.revoke(&key1.id).await.unwrap();

    let keys = store.list("t1").await.unwrap();
    assert_eq!(keys.len(), 1);
}

#[tokio::test]
async fn verify_expired_key_returns_unauthorized() {
    let store = test_store().await;
    let created = store
        .create(&CreateKeyRequest {
            tenant_id: "t1".into(),
            name: "Expiring key".into(),
            scopes: vec![],
            expires_at: Some("2020-01-01T00:00:00.000Z".into()),
        })
        .await
        .unwrap();

    let err = store.verify(&created.raw_token).await.unwrap_err();
    assert_eq!(err.status(), http::StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn refresh_updates_expires_at() {
    let store = test_store().await;
    let created = store
        .create(&CreateKeyRequest {
            tenant_id: "t1".into(),
            name: "Refresh test".into(),
            scopes: vec![],
            expires_at: Some("2020-01-01T00:00:00.000Z".into()),
        })
        .await
        .unwrap();

    // Key is expired — verify fails
    assert!(store.verify(&created.raw_token).await.is_err());

    // Refresh to a future date
    store
        .refresh(&created.id, Some("2099-12-31T23:59:59.000Z"))
        .await
        .unwrap();

    // Now verify succeeds
    let meta = store.verify(&created.raw_token).await.unwrap();
    assert_eq!(
        meta.expires_at.as_deref(),
        Some("2099-12-31T23:59:59.000Z")
    );
}

#[tokio::test]
async fn refresh_to_none_makes_lifetime() {
    let store = test_store().await;
    let created = store
        .create(&CreateKeyRequest {
            tenant_id: "t1".into(),
            name: "Lifetime test".into(),
            scopes: vec![],
            expires_at: Some("2099-12-31T23:59:59.000Z".into()),
        })
        .await
        .unwrap();

    store.refresh(&created.id, None).await.unwrap();

    let meta = store.verify(&created.raw_token).await.unwrap();
    assert!(meta.expires_at.is_none());
}

#[tokio::test]
async fn refresh_nonexistent_returns_not_found() {
    let store = test_store().await;

    let err = store
        .refresh("nonexistent", Some("2099-12-31T23:59:59.000Z"))
        .await
        .unwrap_err();
    assert_eq!(err.status(), http::StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn create_empty_tenant_id_returns_bad_request() {
    let store = test_store().await;

    let err = store
        .create(&CreateKeyRequest {
            tenant_id: "".into(),
            name: "test".into(),
            scopes: vec![],
            expires_at: None,
        })
        .await
        .unwrap_err();
    assert_eq!(err.status(), http::StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn create_empty_name_returns_bad_request() {
    let store = test_store().await;

    let err = store
        .create(&CreateKeyRequest {
            tenant_id: "t1".into(),
            name: "".into(),
            scopes: vec![],
            expires_at: None,
        })
        .await
        .unwrap_err();
    assert_eq!(err.status(), http::StatusCode::BAD_REQUEST);
}
