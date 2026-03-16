use std::sync::Arc;

/// Cached resolved tenant in request extensions.
/// Wraps `Option<Arc<T>>` so that `None` results are also cached,
/// preventing duplicate resolver calls within the same request.
pub(crate) struct ResolvedTenant<T>(pub(crate) Option<Arc<T>>);

impl<T> Clone for ResolvedTenant<T> {
    fn clone(&self) -> Self {
        Self(self.0.as_ref().map(Arc::clone))
    }
}
