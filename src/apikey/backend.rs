use std::future::Future;
use std::pin::Pin;

use crate::error::Result;

use super::types::ApiKeyRecord;

/// Thin storage backend for API keys.
///
/// Implementations handle only CRUD operations. All business logic
/// (key generation, hashing, verification, expiry checks, touch throttling)
/// lives in [`super::ApiKeyStore`].
///
/// The built-in SQLite implementation is in [`super::sqlite`]. Custom
/// backends (Postgres, Redis, etc.) implement this trait directly.
pub trait ApiKeyBackend: Send + Sync {
    /// Store a new key record.
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying storage operation fails.
    fn store(&self, record: &ApiKeyRecord)
    -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>>;

    /// Look up a key by ULID. Returns `None` if not found.
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying storage operation fails.
    fn lookup(
        &self,
        key_id: &str,
    ) -> Pin<Box<dyn Future<Output = Result<Option<ApiKeyRecord>>> + Send + '_>>;

    /// Set `revoked_at` on a key.
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying storage operation fails.
    fn revoke(
        &self,
        key_id: &str,
        revoked_at: &str,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>>;

    /// List all keys for a tenant.
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying storage operation fails.
    fn list(
        &self,
        tenant_id: &str,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<ApiKeyRecord>>> + Send + '_>>;

    /// Update `last_used_at` timestamp.
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying storage operation fails.
    fn update_last_used(
        &self,
        key_id: &str,
        timestamp: &str,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>>;

    /// Update `expires_at` timestamp (refresh).
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying storage operation fails.
    fn update_expires_at(
        &self,
        key_id: &str,
        expires_at: Option<&str>,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>>;
}
