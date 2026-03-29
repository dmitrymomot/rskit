use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::Arc;

/// A point-in-time copy of the service map used internally by middleware and other
/// crate-internal components that need registry access before [`AppState`](super::AppState)
/// is constructed.
///
/// Not part of the public API.
#[derive(Clone)]
pub struct RegistrySnapshot {
    services: HashMap<TypeId, Arc<dyn Any + Send + Sync>>,
}

impl RegistrySnapshot {
    pub(crate) fn new(services: HashMap<TypeId, Arc<dyn Any + Send + Sync>>) -> Self {
        Self { services }
    }

    /// Returns a reference-counted handle to the service stored as type `T`,
    /// or `None` if no such service exists in the snapshot.
    pub fn get<T: Send + Sync + 'static>(&self) -> Option<Arc<T>> {
        self.services
            .get(&TypeId::of::<T>())
            .and_then(|arc| arc.clone().downcast::<T>().ok())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_retrieves_stored_service() {
        let mut map = HashMap::new();
        map.insert(
            TypeId::of::<u32>(),
            Arc::new(42u32) as Arc<dyn Any + Send + Sync>,
        );
        let snap = RegistrySnapshot::new(map);
        let val = snap.get::<u32>().unwrap();
        assert_eq!(*val, 42);
    }

    #[test]
    fn snapshot_returns_none_for_missing() {
        let snap = RegistrySnapshot::new(HashMap::new());
        assert!(snap.get::<String>().is_none());
    }
}
