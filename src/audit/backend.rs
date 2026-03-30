use std::pin::Pin;

use crate::error::Result;

use super::entry::AuditEntry;

/// Object-safe backend trait for audit log storage.
///
/// Implement this trait to use a custom storage backend (e.g., remote
/// logging service, file-based). The built-in SQLite backend is used
/// by [`AuditLog::new()`](super::AuditLog::new).
pub trait AuditLogBackend: Send + Sync {
    /// Persist an audit entry.
    ///
    /// # Errors
    ///
    /// Returns an error if the backend fails to persist the entry.
    fn record<'a>(
        &'a self,
        entry: &'a AuditEntry,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>;
}
