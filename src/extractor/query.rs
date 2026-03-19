use axum::extract::FromRequestParts;
use http::request::Parts;
use serde::de::DeserializeOwned;

use crate::sanitize::Sanitize;

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
