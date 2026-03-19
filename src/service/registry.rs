use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::Arc;

pub struct Registry {
    services: HashMap<TypeId, Arc<dyn Any + Send + Sync>>,
}

impl Registry {
    pub fn new() -> Self {
        Self {
            services: HashMap::new(),
        }
    }

    pub fn add<T: Send + Sync + 'static>(&mut self, service: T) {
        self.services.insert(TypeId::of::<T>(), Arc::new(service));
    }

    pub fn get<T: Send + Sync + 'static>(&self) -> Option<Arc<T>> {
        self.services
            .get(&TypeId::of::<T>())
            .and_then(|arc| arc.clone().downcast::<T>().ok())
    }

    pub(crate) fn into_inner(self) -> HashMap<TypeId, Arc<dyn Any + Send + Sync>> {
        self.services
    }
}

impl Default for Registry {
    fn default() -> Self {
        Self::new()
    }
}
