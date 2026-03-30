//! # modo::audit
//!
//! Explicit audit logging for business-significant actions.
//!
//! Requires feature `"db"`.
//!
//! Records structured events with actor, action, resource, and optional
//! metadata/client context. No automatic middleware capture — callers
//! build an [`AuditEntry`] and pass it to [`AuditLog`]. A built-in
//! SQLite backend writes to the `audit_log` table; custom backends
//! implement [`AuditLogBackend`].
//!
//! | Type | Purpose |
//! |------|---------|
//! | [`AuditEntry`] | Builder for audit events — four required fields plus optional metadata, client info, tenant |
//! | [`AuditRecord`] | Stored row returned by queries — all fields flat, includes `id` and `created_at` |
//! | [`AuditLogBackend`] | Object-safe trait for custom storage backends |
//! | [`AuditLog`] | Service wrapper — [`record()`](AuditLog::record) propagates errors, [`record_silent()`](AuditLog::record_silent) traces and swallows |
//! | [`AuditRepo`] | Query interface — [`list()`](AuditRepo::list) for all entries, [`query()`](AuditRepo::query) with [`ValidatedFilter`](crate::db::ValidatedFilter) |
//! | [`MemoryAuditBackend`] | In-memory backend for tests (requires `test-helpers` feature or `#[cfg(test)]`) |
//!
//! ## Quick start
//!
//! ```rust,no_run
//! use modo::audit::{AuditEntry, AuditLog, AuditRepo};
//! use modo::db::Database;
//!
//! # async fn example(db: Database) -> modo::Result<()> {
//! // Write
//! let audit = AuditLog::new(db.clone());
//! let entry = AuditEntry::new("user_123", "doc.deleted", "document", "doc_42")
//!     .metadata(serde_json::json!({"reason": "expired"}))
//!     .tenant_id("tenant_1");
//! audit.record(&entry).await?;
//!
//! // Query
//! use modo::db::CursorRequest;
//! let repo = AuditRepo::new(db);
//! let page = repo.list(CursorRequest { after: None, per_page: 20 }).await?;
//! # Ok(())
//! # }
//! ```

mod backend;
mod entry;
mod log;
mod record;
mod repo;

pub use self::log::AuditLog;
#[cfg(any(test, feature = "test-helpers"))]
pub use self::log::MemoryAuditBackend;
pub use backend::AuditLogBackend;
pub use entry::AuditEntry;
pub use record::AuditRecord;
pub use repo::AuditRepo;
