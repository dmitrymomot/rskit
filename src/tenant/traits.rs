use std::future::Future;

use crate::Result;

use super::TenantId;

/// Resolved tenant type must implement this to provide identity for tracing.
pub trait HasTenantId {
    /// Returns the tenant's unique identifier for tracing spans.
    fn tenant_id(&self) -> &str;
}

/// Extracts a `TenantId` from an HTTP request.
pub trait TenantStrategy: Send + Sync + 'static {
    /// Extract tenant identifier from request parts.
    /// Takes `&mut Parts` to allow URI rewriting (used by `PathPrefixStrategy`).
    fn extract(&self, parts: &mut http::request::Parts) -> Result<TenantId>;
}

/// Resolves a `TenantId` to an app-defined tenant type.
/// Uses RPITIT — not object-safe; resolver is a concrete type.
pub trait TenantResolver: Send + Sync + 'static {
    /// The resolved tenant type.
    type Tenant: HasTenantId + Send + Sync + Clone + 'static;

    /// Look up a tenant by the extracted identifier.
    fn resolve(&self, id: &TenantId) -> impl Future<Output = Result<Self::Tenant>> + Send;
}
