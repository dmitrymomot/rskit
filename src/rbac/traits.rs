use std::future::Future;

use crate::Result;

/// Resolves the authenticated user's role from an incoming HTTP request.
///
/// Implement this trait on a concrete type and pass it to [`super::middleware()`].
/// The method receives mutable access to request parts so it can call axum
/// extractors such as `Session` internally.
///
/// This trait uses RPITIT and is **not** object-safe. Use it as a concrete type
/// parameter, never as `dyn RoleExtractor`.
pub trait RoleExtractor: Send + Sync + 'static {
    /// Extracts the role string for the current request.
    ///
    /// Return an [`Error`](crate::Error) (e.g., `Error::unauthorized`) to short-circuit
    /// the request. The middleware converts the error into an HTTP response immediately
    /// without forwarding to the inner service.
    fn extract(
        &self,
        parts: &mut http::request::Parts,
    ) -> impl Future<Output = Result<String>> + Send;
}
