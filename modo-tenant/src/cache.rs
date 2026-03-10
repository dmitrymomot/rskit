use std::sync::Arc;

/// Cached resolved tenant in request extensions.
pub(crate) struct ResolvedTenant<T>(pub(crate) Arc<T>);

impl<T> Clone for ResolvedTenant<T> {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}
