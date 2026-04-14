use std::future::Future;

use crate::Result;

/// Resolves the caller's role for the current HTTP request.
///
/// Implement this trait on a concrete type (for example a struct that wraps a
/// [`Database`](crate::db::Database) handle or session helper) and pass an
/// instance to [`super::middleware()`]. The `parts` argument is mutable so
/// implementations can call axum extractors such as
/// [`Session::from_request_parts`](crate::auth::session::Session) or
/// [`Bearer`](crate::auth::Bearer) internally.
///
/// This trait uses return-position `impl Trait` in traits (RPITIT) and is
/// **not** object-safe. Always use it as a generic parameter bound, never as
/// `dyn RoleExtractor` or behind `Box<dyn ...>`.
pub trait RoleExtractor: Send + Sync + 'static {
    /// Extracts the role string for the current request.
    ///
    /// Return an [`Error`](crate::Error) (for example
    /// [`Error::unauthorized`](crate::Error::unauthorized) or
    /// [`Error::forbidden`](crate::Error::forbidden)) to short-circuit the
    /// request. The middleware converts the error into an HTTP response
    /// immediately and does not call the inner service.
    fn extract(
        &self,
        parts: &mut http::request::Parts,
    ) -> impl Future<Output = Result<String>> + Send;
}
