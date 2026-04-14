use axum::extract::FromRequest;
use http::Request;
use serde::de::DeserializeOwned;

use crate::sanitize::Sanitize;

/// Axum extractor that deserializes a JSON request body into `T` and then sanitizes it.
///
/// Returns a 400 Bad Request error if the body is not valid JSON or cannot be
/// deserialized. `T` must implement both [`serde::de::DeserializeOwned`] and
/// [`crate::sanitize::Sanitize`].
///
/// # Example
///
/// ```
/// use modo::extractor::JsonRequest;
/// use modo::sanitize::Sanitize;
/// use serde::Deserialize;
///
/// #[derive(Deserialize)]
/// struct CreateItem { name: String }
///
/// impl Sanitize for CreateItem {
///     fn sanitize(&mut self) { self.name = self.name.trim().to_string(); }
/// }
///
/// async fn create(JsonRequest(body): JsonRequest<CreateItem>) {
///     // body.name is already trimmed
/// }
/// ```
pub struct JsonRequest<T>(pub T);

impl<S, T> FromRequest<S> for JsonRequest<T>
where
    S: Send + Sync,
    T: DeserializeOwned + Sanitize,
{
    type Rejection = crate::error::Error;

    async fn from_request(
        req: Request<axum::body::Body>,
        state: &S,
    ) -> Result<Self, Self::Rejection> {
        let axum::Json(mut value) = axum::Json::<T>::from_request(req, state)
            .await
            .map_err(|e| crate::error::Error::bad_request(format!("invalid JSON: {e}")))?;
        value.sanitize();
        Ok(JsonRequest(value))
    }
}
