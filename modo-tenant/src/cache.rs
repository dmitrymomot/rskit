use std::sync::Arc;

/// Cached resolved tenant in request extensions.
pub struct ResolvedTenant<T>(pub Arc<T>);

impl<T> Clone for ResolvedTenant<T> {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

/// Cached resolved member in request extensions.
pub struct ResolvedMember<M>(pub Arc<M>);

impl<M> Clone for ResolvedMember<M> {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

/// Cached resolved role in request extensions.
#[derive(Clone)]
pub struct ResolvedRole(pub String);

/// Cached resolved tenants list in request extensions.
pub struct ResolvedTenants<T>(pub Arc<Vec<T>>);

impl<T> Clone for ResolvedTenants<T> {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}
