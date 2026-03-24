use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::Arc;

use super::Registry;

/// Immutable, cheaply-cloneable application state produced by [`Registry::into_state`].
///
/// `AppState` is the value passed to [`Router::with_state`](axum::Router::with_state).
/// axum clones it for every request, so the underlying service map is wrapped in an
/// [`Arc`] and never copied.
///
/// Retrieve individual services inside handlers through the
/// [`Service<T>`](crate::extractor::Service) extractor, which calls
/// [`AppState::get`] internally.
#[derive(Clone)]
pub struct AppState {
    services: Arc<HashMap<TypeId, Arc<dyn Any + Send + Sync>>>,
}

impl AppState {
    /// Returns a reference-counted handle to the service registered as type `T`,
    /// or `None` if no such service exists.
    pub fn get<T: Send + Sync + 'static>(&self) -> Option<Arc<T>> {
        self.services
            .get(&TypeId::of::<T>())
            .and_then(|arc| arc.clone().downcast::<T>().ok())
    }
}

impl From<Registry> for AppState {
    /// Converts a [`Registry`] into an [`AppState`] by freezing the service map.
    fn from(registry: Registry) -> Self {
        Self {
            services: Arc::new(registry.into_inner()),
        }
    }
}
