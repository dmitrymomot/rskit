use std::future::Future;

use crate::Result;

use super::TenantId;

/// Required bound on the resolved tenant type.
///
/// Provides the string value recorded into the `tenant_id` tracing field by
/// [`TenantMiddleware`](super::TenantMiddleware) after a successful resolve.
pub trait HasTenantId {
    /// Returns the tenant's unique identifier for tracing spans.
    fn tenant_id(&self) -> &str;
}

/// Extracts a [`TenantId`] from an HTTP request.
///
/// Takes `&mut Parts` so strategies like [`PathPrefixStrategy`](super::PathPrefixStrategy)
/// can rewrite the URI.
pub trait TenantStrategy: Send + Sync + 'static {
    /// Extract a tenant identifier from request parts.
    ///
    /// # Errors
    ///
    /// Returns [`Error`](crate::Error) (typically 400 Bad Request) when the
    /// identifier cannot be extracted (missing header, wrong host, etc.).
    fn extract(&self, parts: &mut http::request::Parts) -> Result<TenantId>;
}

/// Resolves a [`TenantId`] to an app-defined tenant type.
///
/// Uses RPITIT so it is **not** object-safe; the resolver must be a concrete type.
pub trait TenantResolver: Send + Sync + 'static {
    /// The resolved tenant type.
    type Tenant: HasTenantId + Send + Sync + Clone + 'static;

    /// Look up a tenant by the extracted identifier.
    ///
    /// # Errors
    ///
    /// Returns [`Error`](crate::Error) when the tenant cannot be found or the
    /// lookup fails (e.g., database error).
    fn resolve(&self, id: &TenantId) -> impl Future<Output = Result<Self::Tenant>> + Send;
}
