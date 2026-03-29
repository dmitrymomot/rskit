#![cfg(all(feature = "db", feature = "dns"))]

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
