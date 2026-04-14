use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::Arc;

/// A type-map used to register services before the application starts.
///
/// `Registry` is the mutable builder counterpart to [`AppState`](super::AppState).
/// Add all services during startup, then call [`into_state`](Registry::into_state) to
/// produce the immutable [`AppState`](super::AppState) that axum holds.
///
/// Each type `T` can be registered at most once; a second call to [`add`](Registry::add)
/// with the same type overwrites the previous entry.
///
/// # Example
///
/// ```
/// use modo::service::Registry;
///
/// # struct MyService;
/// # impl MyService { fn new() -> Self { Self } }
/// let mut registry = Registry::new();
/// registry.add(MyService::new());
/// let state = registry.into_state();
/// ```
pub struct Registry {
    services: HashMap<TypeId, Arc<dyn Any + Send + Sync>>,
}

impl Registry {
    /// Creates an empty registry.
    pub fn new() -> Self {
        Self {
            services: HashMap::new(),
        }
    }

    /// Registers `service` under its concrete type `T`.
    ///
    /// If a service of type `T` was already registered, it is replaced.
    pub fn add<T: Send + Sync + 'static>(&mut self, service: T) {
        self.services.insert(TypeId::of::<T>(), Arc::new(service));
    }

    /// Returns a reference-counted handle to the service registered as type `T`,
    /// or `None` if no such service exists.
    ///
    /// Useful for startup validation — to confirm a required service was registered
    /// before the server begins accepting requests.
    pub fn get<T: Send + Sync + 'static>(&self) -> Option<Arc<T>> {
        self.services
            .get(&TypeId::of::<T>())
            .and_then(|arc| arc.clone().downcast::<T>().ok())
    }

    /// Returns a point-in-time snapshot of the registry for internal use.
    pub(crate) fn snapshot(&self) -> Arc<super::RegistrySnapshot> {
        Arc::new(super::RegistrySnapshot::new(self.services.clone()))
    }

    /// Consumes the registry and returns an [`AppState`](super::AppState) suitable for
    /// passing to [`Router::with_state`](axum::Router::with_state).
    pub fn into_state(self) -> super::AppState {
        super::AppState::from(self)
    }

    /// Consumes the registry and yields the underlying service map.
    /// Used by [`AppState`](super::AppState) to freeze the registry.
    pub(crate) fn into_inner(self) -> HashMap<TypeId, Arc<dyn Any + Send + Sync>> {
        self.services
    }
}

impl Default for Registry {
    fn default() -> Self {
        Self::new()
    }
}
