use axum::extract::FromRequest;
use http::Request;
use serde::de::DeserializeOwned;

use crate::sanitize::Sanitize;

/// Axum extractor that deserializes a URL-encoded form body into `T` and then sanitizes it.
///
/// `T` must implement both [`serde::de::DeserializeOwned`] and [`crate::sanitize::Sanitize`].
///
/// # Errors
///
/// The [`FromRequest::Rejection`] is [`crate::Error`]. A `400 Bad Request` is returned if
/// the body is not valid `application/x-www-form-urlencoded` data or cannot be deserialized
/// into `T`. The error renders via [`crate::Error::into_response`].
///
/// # Example
///
/// ```rust,no_run
/// use modo::extractor::FormRequest;
/// use modo::sanitize::Sanitize;
/// use serde::Deserialize;
///
/// #[derive(Deserialize)]
/// struct LoginForm { username: String, password: String }
///
/// impl Sanitize for LoginForm {
///     fn sanitize(&mut self) { self.username = self.username.trim().to_lowercase(); }
/// }
///
/// async fn login(FormRequest(form): FormRequest<LoginForm>) {
///     // form.username is already trimmed and lowercased
/// }
/// ```
pub struct FormRequest<T>(pub T);

impl<S, T> FromRequest<S> for FormRequest<T>
where
    S: Send + Sync,
    T: DeserializeOwned + Sanitize,
{
    type Rejection = crate::error::Error;

    async fn from_request(
        req: Request<axum::body::Body>,
        state: &S,
    ) -> Result<Self, Self::Rejection> {
        let axum::Form(mut value) = axum::Form::<T>::from_request(req, state)
            .await
            .map_err(|e| crate::error::Error::bad_request(format!("invalid form data: {e}")))?;
        value.sanitize();
        Ok(FormRequest(value))
    }
}
