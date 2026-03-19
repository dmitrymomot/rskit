use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::Arc;

use super::Registry;

#[derive(Clone)]
pub struct AppState {
    services: Arc<HashMap<TypeId, Arc<dyn Any + Send + Sync>>>,
}

impl AppState {
    pub fn get<T: Send + Sync + 'static>(&self) -> Option<Arc<T>> {
        self.services
            .get(&TypeId::of::<T>())
            .and_then(|arc| arc.clone().downcast::<T>().ok())
    }
}

impl From<Registry> for AppState {
    fn from(registry: Registry) -> Self {
        Self {
            services: Arc::new(registry.into_inner()),
        }
    }
}

impl Registry {
    pub fn into_state(self) -> AppState {
        AppState::from(self)
    }
}
