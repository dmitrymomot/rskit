use std::future::Future;
use std::pin::Pin;

use crate::Result;

/// Optional trait for JWT token revocation.
///
/// Implement this against your storage backend (DB, Redis, LruCache, etc.).
/// Register with `JwtLayer::with_revocation()` — middleware skips the check
/// when no backend is registered.
///
/// # Behavior
///
/// - Only called when revocation backend is registered AND token has a `jti` claim
/// - Token without `jti` + registered backend → accepted
/// - `Ok(true)` → token rejected (revoked)
/// - `Ok(false)` → token accepted
/// - `Err(_)` → token rejected (fail-closed)
pub trait Revocation: Send + Sync {
    fn is_revoked(&self, jti: &str) -> Pin<Box<dyn Future<Output = Result<bool>> + Send + '_>>;
}
