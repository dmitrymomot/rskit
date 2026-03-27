use axum::extract::FromRequestParts;
use http::request::Parts;
use serde::de::DeserializeOwned;

use crate::sanitize::Sanitize;

/// Axum extractor that deserializes URL query parameters into `T` and then sanitizes it.
///
/// Returns a 400 Bad Request error if the query string cannot be deserialized into `T`.
/// `T` must implement both [`serde::de::DeserializeOwned`] and [`crate::sanitize::Sanitize`].
///
/// # Example
///
/// ```
/// use modo::extractor::Query;
/// use modo::Sanitize;
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
