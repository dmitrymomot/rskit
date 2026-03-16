use crate::app::AppState;
use crate::error::Error;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use std::ops::Deref;
use std::sync::Arc;

/// Extractor that retrieves a service from the `ServiceRegistry` by type.
///
/// The inner `Arc<T>` is accessible via `Deref` or by destructuring: `Service(my_svc)`.
/// Returns a `500 Internal Server Error` if the service was not registered.
#[derive(Debug, Clone)]
pub struct Service<T: Send + Sync + 'static>(pub Arc<T>);

impl<T: Send + Sync + 'static> Deref for Service<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T: Send + Sync + 'static> FromRequestParts<AppState> for Service<T> {
    type Rejection = Error;

    async fn from_request_parts(
        _parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        state.services.get::<T>().map(Service).ok_or_else(|| {
            Error::internal(format!(
                "service not registered: {}",
                std::any::type_name::<T>()
            ))
        })
    }
}
