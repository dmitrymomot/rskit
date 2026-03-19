use axum::extract::FromRequest;
use http::Request;
use serde::de::DeserializeOwned;

use crate::sanitize::Sanitize;

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
