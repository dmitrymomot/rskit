use axum::body::{Body, to_bytes};
use axum::extract::FromRequest;
use http::{Request, header};
use serde::de::DeserializeOwned;

use crate::sanitize::Sanitize;

/// Axum extractor that deserializes a URL-encoded form body into `T` and then sanitizes it.
///
/// `T` must implement both [`serde::de::DeserializeOwned`] and [`crate::sanitize::Sanitize`].
///
/// Repeated form keys, nested structs, and `Vec<Struct>` rows all deserialize via `serde_qs`
/// form-encoding mode. Flat repeats (`tag=a&tag=b`) populate `Vec<scalar>` fields,
/// `client[name]=…` populates a nested struct, and indexed brackets (`contacts[0][kind]=…`)
/// populate `Vec<Struct>` rows. For per-row dynamic forms, the indexed form is required so
/// the deserializer can group fields into the correct row.
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
/// struct Contact { kind: String, value: String, comment: String }
///
/// #[derive(Deserialize)]
/// struct NewClient {
///     name: String,
///     work_days: Vec<u8>,        // multi-select checkbox group
///     contacts: Vec<Contact>,    // contacts[0][kind]=…&contacts[0][value]=…
/// }
///
/// impl Sanitize for NewClient {
///     fn sanitize(&mut self) { self.name = self.name.trim().to_string(); }
/// }
///
/// async fn create(FormRequest(form): FormRequest<NewClient>) {
///     // form.contacts has one entry per submitted row
/// }
/// ```
pub struct FormRequest<T>(pub T);

impl<S, T> FromRequest<S> for FormRequest<T>
where
    S: Send + Sync,
    T: DeserializeOwned + Sanitize,
{
    type Rejection = crate::error::Error;

    async fn from_request(req: Request<Body>, _state: &S) -> Result<Self, Self::Rejection> {
        if !has_form_content_type(&req) {
            return Err(crate::error::Error::bad_request(
                "expected `application/x-www-form-urlencoded` content type",
            ));
        }

        let bytes = to_bytes(req.into_body(), usize::MAX)
            .await
            .map_err(|e| crate::error::Error::bad_request(format!("failed to read body: {e}")))?;

        let mut value: T = serde_qs::Config::new()
            .use_form_encoding(true)
            .deserialize_bytes(&bytes)
            .map_err(|e| crate::error::Error::bad_request(format!("invalid form data: {e}")))?;
        value.sanitize();
        Ok(FormRequest(value))
    }
}

fn has_form_content_type<B>(req: &Request<B>) -> bool {
    let Some(value) = req.headers().get(header::CONTENT_TYPE) else {
        return false;
    };
    let Ok(text) = value.to_str() else {
        return false;
    };
    let mime_type = text.split(';').next().unwrap_or("").trim();
    mime_type.eq_ignore_ascii_case("application/x-www-form-urlencoded")
}
