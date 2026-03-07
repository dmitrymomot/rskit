use crate::types::JobId;
use modo::app::ServiceRegistry;
use modo_db::pool::DbPool;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// Context passed to job handler functions.
pub struct JobContext {
    pub job_id: JobId,
    pub name: String,
    pub queue: String,
    pub attempt: i32,
    pub(crate) services: ServiceRegistry,
    pub(crate) db: Option<DbPool>,
    pub(crate) payload_json: String,
}

impl JobContext {
    /// Deserialize the job payload from JSON.
    pub fn payload<T: serde::de::DeserializeOwned>(&self) -> Result<T, modo::Error> {
        serde_json::from_str(&self.payload_json)
            .map_err(|e| modo::Error::internal(format!("Failed to deserialize job payload: {e}")))
    }

    /// Retrieve a service from the registry.
    pub fn service<T: Send + Sync + 'static>(&self) -> Result<Arc<T>, modo::Error> {
        self.services.get::<T>().ok_or_else(|| {
            modo::Error::internal(format!(
                "Service not registered: {}",
                std::any::type_name::<T>()
            ))
        })
    }

    /// Get the database connection pool.
    pub fn db(&self) -> Result<&DbPool, modo::Error> {
        self.db
            .as_ref()
            .ok_or_else(|| modo::Error::internal("Database not available in job context"))
    }
}

/// Trait for job handlers (user-facing, async fn friendly).
pub trait JobHandler: Send + Sync + 'static {
    fn run(&self, ctx: JobContext) -> impl Future<Output = Result<(), modo::Error>> + Send;
}

/// Object-safe version of `JobHandler`.
pub trait JobHandlerDyn: Send + Sync + 'static {
    fn run_dyn(
        &self,
        ctx: JobContext,
    ) -> Pin<Box<dyn Future<Output = Result<(), modo::Error>> + Send + '_>>;
}

impl<T: JobHandler> JobHandlerDyn for T {
    fn run_dyn(
        &self,
        ctx: JobContext,
    ) -> Pin<Box<dyn Future<Output = Result<(), modo::Error>> + Send + '_>> {
        Box::pin(self.run(ctx))
    }
}

/// Registration entry for a job handler, collected via `inventory`.
pub struct JobRegistration {
    pub name: &'static str,
    pub queue: &'static str,
    pub priority: i32,
    pub max_attempts: u32,
    pub timeout_secs: u64,
    pub cron: Option<&'static str>,
    pub handler_factory: fn() -> Box<dyn JobHandlerDyn>,
}

inventory::collect!(JobRegistration);
