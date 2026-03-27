use std::pin::Pin;

use crate::Result;

/// Optional trait for JWT token revocation checks.
///
/// Implement this against your storage backend (DB, Redis, `LruCache`, etc.).
/// Register with [`JwtLayer::with_revocation()`](super::middleware::JwtLayer::with_revocation) —
/// the middleware skips the check when no backend is registered.
///
/// # Behavior
///
/// - Only called when a revocation backend is registered AND the token has a `jti` claim.
/// - Token without `jti` + registered backend: accepted without calling `is_revoked`.
/// - `Ok(true)`: token rejected with `jwt:revoked`.
/// - `Ok(false)`: token accepted.
/// - `Err(_)`: token rejected with `jwt:revocation_check_failed` (fail-closed).
pub trait Revocation: Send + Sync {
    /// Returns `Ok(true)` if the token identified by `jti` has been revoked.
    fn is_revoked(&self, jti: &str) -> Pin<Box<dyn Future<Output = Result<bool>> + Send + '_>>;
}
