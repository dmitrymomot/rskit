use std::future::Future;

use crate::Result;

/// Extracts a role string from an HTTP request.
/// App implements this to resolve the authenticated user's role.
/// Takes `&mut Parts` so the extractor can call axum's `FromRequestParts`
/// extractors (e.g., `Session`) if needed.
/// Uses RPITIT — not object-safe; extractor is a concrete type.
pub trait RoleExtractor: Send + Sync + 'static {
    fn extract(
        &self,
        parts: &mut http::request::Parts,
    ) -> impl Future<Output = Result<String>> + Send;
}
