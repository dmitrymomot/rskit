use axum::extract::FromRequestParts;
use http::request::Parts;
use serde::de::DeserializeOwned;

use crate::sanitize::Sanitize;

/// Axum extractor that deserializes URL query parameters into `T` and then sanitizes it.
///
/// `T` must implement both [`serde::de::DeserializeOwned`] and [`crate::sanitize::Sanitize`].
///
/// Because this extractor implements [`FromRequestParts`] rather than `FromRequest`, it
/// can be combined with body extractors on the same handler. To make `Query` optional
/// (i.e. `Option<Query<T>>`), axum 0.8 requires an explicit `OptionalFromRequestParts`
/// impl — this crate does not provide one, so use a type whose fields are `Option<_>`
/// instead.
///
/// # Errors
///
/// The [`FromRequestParts::Rejection`] is [`crate::Error`]. A `400 Bad Request` is
/// returned if the query string cannot be deserialized into `T`. The error renders via
/// [`crate::Error::into_response`].
///
/// # Example
///
/// ```rust,no_run
/// use modo::extractor::Query;
/// use modo::sanitize::Sanitize;
/// use serde::Deserialize;
///
/// #[derive(Deserialize)]
/// struct SearchParams { q: String, page: Option<u32> }
///
/// impl Sanitize for SearchParams {
///     fn sanitize(&mut self) { self.q = self.q.trim().to_lowercase(); }
/// }
///
/// async fn search(Query(params): Query<SearchParams>) {
///     // params.q is already trimmed and lowercased
/// }
/// ```
pub struct Query<T>(pub T);

impl<S, T> FromRequestParts<S> for Query<T>
where
    S: Send + Sync,
    T: DeserializeOwned + Sanitize,
{
    type Rejection = crate::error::Error;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let axum::extract::Query(mut value) =
            axum::extract::Query::<T>::from_request_parts(parts, state)
                .await
                .map_err(|e| crate::error::Error::bad_request(format!("invalid query: {e}")))?;
        value.sanitize();
        Ok(Query(value))
    }
}
