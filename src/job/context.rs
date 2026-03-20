use std::sync::Arc;

use serde::de::DeserializeOwned;

use crate::error::{Error, Result};
use crate::extractor::Service;
use crate::service::RegistrySnapshot;

use super::meta::Meta;
use super::payload::Payload;

pub struct JobContext {
    pub(crate) registry: Arc<RegistrySnapshot>,
    pub(crate) payload: String,
    pub(crate) meta: Meta,
}

pub trait FromJobContext: Sized {
    fn from_job_context(ctx: &JobContext) -> Result<Self>;
}

impl<T: DeserializeOwned> FromJobContext for Payload<T> {
    fn from_job_context(ctx: &JobContext) -> Result<Self> {
        let value: T = serde_json::from_str(&ctx.payload).map_err(|e| {
            Error::internal(format!(
                "failed to deserialize job payload for '{}': {e}",
                ctx.meta.name
            ))
        })?;
        Ok(Payload(value))
    }
}

impl<T: Send + Sync + 'static> FromJobContext for Service<T> {
    fn from_job_context(ctx: &JobContext) -> Result<Self> {
        ctx.registry.get::<T>().map(Service).ok_or_else(|| {
            Error::internal(format!(
                "service not found in registry: {}",
                std::any::type_name::<T>()
            ))
        })
    }
}

impl FromJobContext for Meta {
    fn from_job_context(ctx: &JobContext) -> Result<Self> {
        Ok(ctx.meta.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::any::{Any, TypeId};
    use std::collections::HashMap;

    fn test_context(payload: &str) -> JobContext {
        let mut services: HashMap<TypeId, Arc<dyn Any + Send + Sync>> = HashMap::new();
        services.insert(TypeId::of::<String>(), Arc::new("test-service".to_string()));
        let snapshot = Arc::new(RegistrySnapshot::new(services));

        JobContext {
            registry: snapshot,
            payload: payload.to_string(),
            meta: Meta {
                id: "test-id".to_string(),
                name: "test-job".to_string(),
                queue: "default".to_string(),
                attempt: 1,
                max_attempts: 3,
                deadline: None,
            },
        }
    }

    #[test]
    fn payload_extractor_deserializes_json() {
        let ctx = test_context(r#"{"value": 42}"#);

        #[derive(serde::Deserialize)]
        struct TestPayload {
            value: u32,
        }

        let payload = Payload::<TestPayload>::from_job_context(&ctx).unwrap();
        assert_eq!(payload.value, 42);
    }

    #[test]
    fn payload_extractor_fails_on_invalid_json() {
        let ctx = test_context("not json");
        let result = Payload::<serde_json::Value>::from_job_context(&ctx);
        assert!(result.is_err());
    }

    #[test]
    fn service_extractor_finds_registered() {
        let ctx = test_context("{}");
        let svc = Service::<String>::from_job_context(&ctx).unwrap();
        assert_eq!(*svc.0, "test-service");
    }

    #[test]
    fn service_extractor_fails_for_missing() {
        let ctx = test_context("{}");
        let result = Service::<u64>::from_job_context(&ctx);
        assert!(result.is_err());
    }

    #[test]
    fn meta_extractor_clones_meta() {
        let ctx = test_context("{}");
        let meta = Meta::from_job_context(&ctx).unwrap();
        assert_eq!(meta.id, "test-id");
        assert_eq!(meta.name, "test-job");
        assert_eq!(meta.attempt, 1);
    }
}
