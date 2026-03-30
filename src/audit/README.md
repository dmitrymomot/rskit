# modo::audit

Explicit audit logging for business-significant actions.

Records structured events with actor, action, resource type, and resource
ID. Optional metadata (arbitrary JSON), client context (IP, user-agent,
fingerprint), and tenant ID round out each entry. No automatic middleware
capture -- callers build an `AuditEntry` and pass it to `AuditLog`.

Requires the **`db`** feature flag (enabled by default).

```toml
[dependencies]
modo = { version = "0.2", features = ["db"] }
```

## Schema

The application must create the `audit_log` table before recording events.
modo does not ship migrations -- end-apps own their schemas.

```sql
CREATE TABLE IF NOT EXISTS audit_log (
    id            TEXT NOT NULL PRIMARY KEY,
    actor         TEXT NOT NULL,
    action        TEXT NOT NULL,
    resource_type TEXT NOT NULL,
    resource_id   TEXT NOT NULL,
    metadata      TEXT NOT NULL DEFAULT '{}',
    ip            TEXT,
    user_agent    TEXT,
    fingerprint   TEXT,
    tenant_id     TEXT,
    created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);
CREATE INDEX IF NOT EXISTS idx_audit_log_actor         ON audit_log (actor);
CREATE INDEX IF NOT EXISTS idx_audit_log_action        ON audit_log (action);
CREATE INDEX IF NOT EXISTS idx_audit_log_resource      ON audit_log (resource_type, resource_id);
CREATE INDEX IF NOT EXISTS idx_audit_log_tenant_id     ON audit_log (tenant_id);
CREATE INDEX IF NOT EXISTS idx_audit_log_created_at    ON audit_log (created_at);
```

## Key types

| Type | Purpose |
|------|---------|
| `AuditEntry` | Builder for audit events -- four required fields plus optional metadata, client info, tenant |
| `AuditRecord` | Stored row returned by queries -- all fields flat, includes `id` and `created_at` |
| `AuditLogBackend` | Object-safe trait for custom storage backends |
| `AuditLog` | Service wrapper -- `record()` propagates errors, `record_silent()` traces and swallows |
| `AuditRepo` | Query interface -- `list()` for all entries, `query()` with `ValidatedFilter` |
| `MemoryAuditBackend` | In-memory backend for tests (requires `audit-test` feature or `#[cfg(test)]`) |

## Usage

### Recording events

```rust,no_run
use modo::audit::{AuditEntry, AuditLog};
use modo::db::Database;

async fn delete_document(db: Database, audit: AuditLog) -> modo::Result<()> {
    // ... delete logic ...

    let entry = AuditEntry::new("user_123", "doc.deleted", "document", "doc_42")
        .metadata(serde_json::json!({"reason": "expired"}))
        .tenant_id("tenant_1");

    // Propagate errors
    audit.record(&entry).await?;

    // Or swallow errors (logs via tracing::error)
    // audit.record_silent(&entry).await;

    Ok(())
}
```

### Attaching client context

```rust,no_run
use modo::audit::AuditEntry;
use modo::extractor::ClientInfo;

let info = ClientInfo::new()
    .ip("203.0.113.42")
    .user_agent("Mozilla/5.0");

let entry = AuditEntry::new("user_123", "user.login", "session", "sess_abc")
    .client_info(info);
```

### Querying records

```rust,no_run
use modo::audit::{AuditRepo, AuditRecord};
use modo::db::{CursorRequest, Database};

async fn list_events(db: Database) -> modo::Result<()> {
    let repo = AuditRepo::new(db);
    let page = repo.list(CursorRequest::default()).await?;

    for record in &page.items {
        println!("{}: {} by {}", record.action, record.resource_id, record.actor);
    }
    Ok(())
}
```

### Custom backend

Implement `AuditLogBackend` to route events to a different store:

```rust,no_run
use std::pin::Pin;
use std::sync::Arc;
use modo::audit::{AuditLogBackend, AuditEntry, AuditLog};

struct MyBackend;

impl AuditLogBackend for MyBackend {
    fn record<'a>(
        &'a self,
        entry: &'a AuditEntry,
    ) -> Pin<Box<dyn Future<Output = modo::Result<()>> + Send + 'a>> {
        Box::pin(async move {
            // custom storage logic
            Ok(())
        })
    }
}

let audit = AuditLog::from_backend(Arc::new(MyBackend));
```

### Testing

Enable the `audit-test` feature for access to `MemoryAuditBackend`:

```rust,ignore
use modo::audit::{AuditEntry, AuditLog, MemoryAuditBackend};

let (audit, backend) = AuditLog::memory();

let entry = AuditEntry::new("user_1", "doc.created", "document", "doc_1");
audit.record(&entry).await.unwrap();

let captured = backend.entries();
assert_eq!(captured.len(), 1);
assert_eq!(captured[0].action(), "doc.created");
```

## Error handling

`AuditLog::record()` returns `modo::Result<()>` -- errors propagate through
the standard `modo::Error` type. Use `record_silent()` when audit failures
should not break the primary request flow; it logs the error via
`tracing::error` and returns `()`.
