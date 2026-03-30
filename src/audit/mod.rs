//! Audit logging for business-significant actions.
//!
//! Provides explicit event recording (no automatic middleware capture) with
//! a pluggable backend trait and a built-in SQLite implementation.
//!
//! | Type | Purpose |
//! |---|---|
//! | [`AuditEntry`] | Builder for audit events — four required fields plus optional metadata, client info, tenant |
//! | [`AuditRecord`] | Stored form returned by queries — all fields flat, includes `id` and `created_at` |
//! | [`AuditLogBackend`] | Object-safe trait for custom storage backends |
//! | [`AuditLog`] | Concrete wrapper — `record()` propagates errors, `record_silent()` traces and swallows |
//! | [`AuditRepo`] | Query interface — dedicated methods plus generic filter-based `query()` |

mod backend;
mod entry;
mod log;
mod record;

pub use self::log::AuditLog;
#[cfg(any(test, feature = "audit-test"))]
pub use self::log::MemoryAuditBackend;
pub use backend::AuditLogBackend;
pub use entry::AuditEntry;
pub use record::AuditRecord;
