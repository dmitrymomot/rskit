#![cfg(feature = "db")]

use modo::audit::{AuditEntry, AuditLog, AuditRepo};
use modo::db::PageRequest;
use modo::extractor::ClientInfo;
use modo::testing::TestDb;

const SCHEMA: &str = "\
CREATE TABLE audit_log (
    id              TEXT PRIMARY KEY,
    actor           TEXT NOT NULL,
    action          TEXT NOT NULL,
    resource_type   TEXT NOT NULL,
    resource_id     TEXT NOT NULL,
    metadata        TEXT NOT NULL DEFAULT '{}',
    ip              TEXT,
    user_agent      TEXT,
    fingerprint     TEXT,
    tenant_id       TEXT,
    created_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
)";

async fn setup() -> (AuditLog, AuditRepo) {
    let db = TestDb::new().await.exec(SCHEMA).await.db();
    (AuditLog::new(db.clone()), AuditRepo::new(db))
}

fn page(page: i64, per_page: i64) -> PageRequest {
    PageRequest { page, per_page }
}

#[tokio::test]
async fn record_and_read_back() {
    let (log, repo) = setup().await;

    log.record(
        &AuditEntry::new("user_1", "user.created", "user", "usr_abc")
            .metadata(serde_json::json!({"source": "signup"}))
            .client_info(ClientInfo::new().ip("1.2.3.4").user_agent("TestBot/1.0"))
            .tenant_id("t_1"),
    )
    .await
    .unwrap();

    let result = repo.list(&page(1, 10)).await.unwrap();
    assert_eq!(result.total, 1);
    assert_eq!(result.items.len(), 1);

    let record = &result.items[0];
    assert_eq!(record.actor, "user_1");
    assert_eq!(record.action, "user.created");
    assert_eq!(record.resource_type, "user");
    assert_eq!(record.resource_id, "usr_abc");
    assert_eq!(record.metadata["source"], "signup");
    assert_eq!(record.ip.as_deref(), Some("1.2.3.4"));
    assert_eq!(record.user_agent.as_deref(), Some("TestBot/1.0"));
    assert!(record.fingerprint.is_none());
    assert_eq!(record.tenant_id.as_deref(), Some("t_1"));
    assert!(!record.id.is_empty());
    assert!(!record.created_at.is_empty());
}

#[tokio::test]
async fn record_without_optional_fields() {
    let (log, repo) = setup().await;

    log.record(&AuditEntry::new("system", "job.ran", "job", "job_1"))
        .await
        .unwrap();

    let result = repo.list(&page(1, 10)).await.unwrap();
    let record = &result.items[0];
    assert_eq!(record.actor, "system");
    assert_eq!(record.metadata, serde_json::json!({}));
    assert!(record.ip.is_none());
    assert!(record.user_agent.is_none());
    assert!(record.fingerprint.is_none());
    assert!(record.tenant_id.is_none());
}

#[tokio::test]
async fn record_silent_does_not_panic() {
    let (log, _) = setup().await;
    log.record_silent(&AuditEntry::new("system", "test.ok", "test", "t_1"))
        .await;
}

#[tokio::test]
async fn by_actor_filters() {
    let (log, repo) = setup().await;

    log.record(&AuditEntry::new("user_1", "a.1", "x", "x1")).await.unwrap();
    log.record(&AuditEntry::new("user_2", "a.2", "x", "x2")).await.unwrap();
    log.record(&AuditEntry::new("user_1", "a.3", "x", "x3")).await.unwrap();

    let result = repo.by_actor("user_1", &page(1, 10)).await.unwrap();
    assert_eq!(result.total, 2);
    assert!(result.items.iter().all(|r| r.actor == "user_1"));
}

#[tokio::test]
async fn by_resource_filters() {
    let (log, repo) = setup().await;

    log.record(&AuditEntry::new("u", "a", "user", "usr_1")).await.unwrap();
    log.record(&AuditEntry::new("u", "a", "user", "usr_2")).await.unwrap();
    log.record(&AuditEntry::new("u", "a", "doc", "doc_1")).await.unwrap();

    let result = repo.by_resource("user", "usr_1", &page(1, 10)).await.unwrap();
    assert_eq!(result.total, 1);
    assert_eq!(result.items[0].resource_id, "usr_1");
}

#[tokio::test]
async fn by_tenant_filters() {
    let (log, repo) = setup().await;

    log.record(&AuditEntry::new("u", "a", "x", "x1").tenant_id("t_1")).await.unwrap();
    log.record(&AuditEntry::new("u", "a", "x", "x2").tenant_id("t_2")).await.unwrap();

    let result = repo.by_tenant("t_1", &page(1, 10)).await.unwrap();
    assert_eq!(result.total, 1);
    assert_eq!(result.items[0].tenant_id.as_deref(), Some("t_1"));
}

#[tokio::test]
async fn by_action_filters() {
    let (log, repo) = setup().await;

    log.record(&AuditEntry::new("u", "user.created", "user", "u1")).await.unwrap();
    log.record(&AuditEntry::new("u", "user.deleted", "user", "u2")).await.unwrap();

    let result = repo.by_action("user.created", &page(1, 10)).await.unwrap();
    assert_eq!(result.total, 1);
    assert_eq!(result.items[0].action, "user.created");
}

#[tokio::test]
async fn pagination_works() {
    let (log, repo) = setup().await;

    for i in 0..5 {
        log.record(&AuditEntry::new("u", format!("a.{i}"), "x", format!("x{i}")))
            .await
            .unwrap();
    }

    let p1 = repo.list(&page(1, 2)).await.unwrap();
    assert_eq!(p1.total, 5);
    assert_eq!(p1.items.len(), 2);
    assert!(p1.has_next);
    assert!(!p1.has_prev);

    let p3 = repo.list(&page(3, 2)).await.unwrap();
    assert_eq!(p3.items.len(), 1);
    assert!(!p3.has_next);
    assert!(p3.has_prev);
}

#[cfg(feature = "audit-test")]
#[tokio::test]
async fn memory_backend_captures_entries() {
    let (log, backend) = AuditLog::memory();

    log.record(&AuditEntry::new("user_1", "test.action", "test", "t_1"))
        .await
        .unwrap();

    let entries = backend.entries();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].actor(), "user_1");
    assert_eq!(entries[0].action(), "test.action");
}
