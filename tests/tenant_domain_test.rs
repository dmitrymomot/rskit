#![cfg(all(feature = "db", feature = "dns"))]

use chrono::Utc;
use modo::db::{self, ConnExt};
use modo::dns::DnsConfig;
use modo::tenant::domain::{ClaimStatus, DomainService};

const CREATE_TABLE_SQL: &str = "\
CREATE TABLE tenant_domains (\
    id TEXT PRIMARY KEY,\
    tenant_id TEXT NOT NULL,\
    domain TEXT NOT NULL,\
    verification_token TEXT NOT NULL,\
    status TEXT NOT NULL DEFAULT 'pending',\
    use_for_email INTEGER NOT NULL DEFAULT 0,\
    use_for_routing INTEGER NOT NULL DEFAULT 0,\
    created_at TEXT NOT NULL,\
    verified_at TEXT\
);\
CREATE UNIQUE INDEX idx_tenant_domains_domain ON tenant_domains(domain) WHERE status = 'verified';\
";

async fn setup() -> DomainService {
    let config = db::Config {
        path: ":memory:".into(),
        ..Default::default()
    };
    let db = db::connect(&config).await.unwrap();
    db.conn().execute_raw(CREATE_TABLE_SQL, ()).await.unwrap();
    let verifier = modo::dns::DomainVerifier::from_config(&DnsConfig::default()).unwrap();
    DomainService::new(db, verifier)
}

/// Create an isolated DB + DomainService pair so tests can access the raw
/// connection to force a domain into `verified` status (bypassing DNS).
async fn setup_with_db() -> (db::Database, DomainService) {
    let config = db::Config {
        path: ":memory:".into(),
        ..Default::default()
    };
    let database = db::connect(&config).await.unwrap();
    database
        .conn()
        .execute_raw(CREATE_TABLE_SQL, ())
        .await
        .unwrap();
    let verifier =
        modo::dns::DomainVerifier::from_config(&modo::dns::DnsConfig::default()).unwrap();
    let svc = DomainService::new(database.clone(), verifier);
    (database, svc)
}

/// Force a domain claim into verified status directly in the DB.
async fn force_verify(database: &db::Database, claim_id: &str) {
    let now = Utc::now().to_rfc3339();
    database
        .conn()
        .execute_raw(
            "UPDATE tenant_domains SET status = 'verified', verified_at = ?1 WHERE id = ?2",
            libsql::params![now.as_str(), claim_id],
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn register_domain() {
    let svc = setup().await;
    let claim = svc.register("tenant-1", "example.com").await.unwrap();
    assert_eq!(claim.tenant_id, "tenant-1");
    assert_eq!(claim.domain, "example.com");
    assert_eq!(claim.status, ClaimStatus::Pending);
    assert!(!claim.verification_token.is_empty());
    assert!(!claim.use_for_email);
    assert!(!claim.use_for_routing);
}

#[tokio::test]
async fn list_domains_for_tenant() {
    let svc = setup().await;
    svc.register("tenant-1", "a.com").await.unwrap();
    svc.register("tenant-1", "b.com").await.unwrap();
    svc.register("tenant-2", "c.com").await.unwrap();
    let list = svc.list("tenant-1").await.unwrap();
    assert_eq!(list.len(), 2);
}

#[tokio::test]
async fn remove_domain() {
    let svc = setup().await;
    let claim = svc.register("tenant-1", "example.com").await.unwrap();
    svc.remove(&claim.id).await.unwrap();
    let list = svc.list("tenant-1").await.unwrap();
    assert!(list.is_empty());
}

#[tokio::test]
async fn invalid_domain_rejected() {
    let svc = setup().await;
    assert!(svc.register("t1", "").await.is_err());
    assert!(svc.register("t1", "nodot").await.is_err());
    assert!(svc.register("t1", ".leading.dot").await.is_err());
}

#[tokio::test]
async fn test_domain_enable_disable_email() {
    let (database, svc) = setup_with_db().await;

    let claim = svc.register("tenant-e", "mail.test.com").await.unwrap();
    force_verify(&database, &claim.id).await;

    // enable_email should succeed on a verified domain.
    svc.enable_email(&claim.id).await.unwrap();

    // lookup_email_domain should now find the tenant.
    let result = svc.lookup_email_domain("user@mail.test.com").await.unwrap();
    assert!(result.is_some());
    let m = result.unwrap();
    assert_eq!(m.tenant_id, "tenant-e");
    assert_eq!(m.domain, "mail.test.com");

    // disable_email should make it invisible again.
    svc.disable_email(&claim.id).await.unwrap();
    let result = svc.lookup_email_domain("user@mail.test.com").await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn test_domain_enable_disable_routing() {
    let (database, svc) = setup_with_db().await;

    let claim = svc.register("tenant-r", "app.test.com").await.unwrap();
    force_verify(&database, &claim.id).await;

    // enable_routing should succeed on a verified domain.
    svc.enable_routing(&claim.id).await.unwrap();

    // lookup_routing_domain should now find the tenant.
    let result = svc.lookup_routing_domain("app.test.com").await.unwrap();
    assert!(result.is_some());
    let m = result.unwrap();
    assert_eq!(m.tenant_id, "tenant-r");
    assert_eq!(m.domain, "app.test.com");

    // disable_routing should make it invisible again.
    svc.disable_routing(&claim.id).await.unwrap();
    let result = svc.lookup_routing_domain("app.test.com").await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn test_domain_duplicate_registration() {
    let svc = setup().await;

    // Register the same domain twice for the same tenant.
    let first = svc.register("tenant-d", "dup.example.com").await.unwrap();
    let second = svc.register("tenant-d", "dup.example.com").await.unwrap();

    // The second call must return the existing pending claim, not create a new row.
    assert_eq!(first.id, second.id);
    assert_eq!(second.status, ClaimStatus::Pending);

    // Only one row should be present.
    let list = svc.list("tenant-d").await.unwrap();
    assert_eq!(list.len(), 1);
}
