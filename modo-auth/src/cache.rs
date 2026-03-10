use std::sync::Arc;

/// Cached resolved user in request extensions.
///
/// Inserted by `UserContextLayer` or auth extractors so that subsequent
/// `Auth<U>` / `OptionalAuth<U>` calls reuse the already-loaded user
/// without a second DB lookup.
pub(crate) struct ResolvedUser<U>(pub(crate) Arc<U>);

impl<U> Clone for ResolvedUser<U> {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}
