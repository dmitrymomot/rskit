use std::sync::Arc;

use crate::error::{Error, Result};
use crate::service::{RegistrySnapshot, Service};

use super::meta::Meta;

/// Execution context passed to every cron handler invocation.
///
/// Carries a snapshot of the service registry and the job metadata for the
/// current tick. This type is constructed by the scheduler and passed to
/// [`CronHandler::call`](super::CronHandler) — handlers do not create it directly. Use
/// [`FromCronContext`] to extract individual values from the context as
/// handler arguments.
pub struct CronContext {
    pub(crate) registry: Arc<RegistrySnapshot>,
    pub(crate) meta: Meta,
}

/// Extracts a value from a [`CronContext`].
///
/// Implement this trait to make a type usable as a cron handler argument.
/// Built-in implementations are provided for [`Service<T>`] and [`Meta`].
pub trait FromCronContext: Sized {
    /// Attempt to extract `Self` from the given context.
    ///
    /// # Errors
    ///
    /// Returns an error if the required data is not present (e.g. a service
    /// was not registered before the scheduler was built).
    fn from_cron_context(ctx: &CronContext) -> Result<Self>;
}

impl<T: Send + Sync + 'static> FromCronContext for Service<T> {
    fn from_cron_context(ctx: &CronContext) -> Result<Self> {
        ctx.registry.get::<T>().map(Service).ok_or_else(|| {
            Error::internal(format!(
                "service not found in registry: {}",
                std::any::type_name::<T>()
            ))
        })
    }
}

impl FromCronContext for Meta {
    fn from_cron_context(ctx: &CronContext) -> Result<Self> {
        Ok(ctx.meta.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::any::{Any, TypeId};
    use std::collections::HashMap;

    fn test_context() -> CronContext {
        let mut services: HashMap<TypeId, Arc<dyn Any + Send + Sync>> = HashMap::new();
        services.insert(TypeId::of::<u32>(), Arc::new(42u32));
        let snapshot = Arc::new(RegistrySnapshot::new(services));

        CronContext {
            registry: snapshot,
            meta: Meta {
                name: "test_job".to_string(),
                deadline: None,
                tick: chrono::Utc::now(),
            },
        }
    }

    #[test]
    fn service_extractor_finds_registered() {
        let ctx = test_context();
        let svc = Service::<u32>::from_cron_context(&ctx).unwrap();
        assert_eq!(*svc.0, 42);
    }

    #[test]
    fn service_extractor_fails_for_missing() {
        let ctx = test_context();
        let result = Service::<String>::from_cron_context(&ctx);
        assert!(result.is_err());
    }

    #[test]
    fn meta_extractor_returns_meta() {
        let ctx = test_context();
        let meta = Meta::from_cron_context(&ctx).unwrap();
        assert_eq!(meta.name, "test_job");
    }
}
