use std::sync::Arc;

use axum::extract::{FromRef, FromRequestParts};
use http::request::Parts;

use crate::service::AppState;

/// Axum extractor that retrieves a service `T` from the application's service registry.
///
/// The inner value is an `Arc<T>`, so cloning the extractor is cheap.
/// Returns a 500 Internal Server Error if `T` was not registered before the server started.
///
/// # Example
///
/// ```ignore
/// use modo::Service;
///
/// struct MyService { /* ... */ }
///
/// async fn handler(Service(svc): Service<MyService>) {
///     // svc is Arc<MyService>
/// }
/// ```
pub struct Service<T>(pub Arc<T>);

impl<S, T> FromRequestParts<S> for Service<T>
where
    S: Send + Sync,
    T: Send + Sync + 'static,
    AppState: FromRef<S>,
{
    type Rejection = crate::error::Error;

    async fn from_request_parts(_parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let app_state = AppState::from_ref(state);

        app_state.get::<T>().map(Service).ok_or_else(|| {
            crate::error::Error::internal(format!(
                "service not found in registry: {}",
                std::any::type_name::<T>()
            ))
        })
    }
}
