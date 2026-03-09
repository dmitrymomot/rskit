use std::sync::Arc;

/// Cached resolved tenant in request extensions.
pub struct ResolvedTenant<T>(pub Arc<T>);

impl<T> Clone for ResolvedTenant<T> {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}
