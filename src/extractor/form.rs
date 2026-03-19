use axum::extract::FromRequest;
use http::Request;
use serde::de::DeserializeOwned;

use crate::sanitize::Sanitize;

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
