use crate::types::JobId;
use modo::app::ServiceRegistry;
use modo_db::pool::DbPool;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// Context passed to a job handler when the job is executed.
///
/// Provides access to the deserialized payload, registered services, and the
/// database connection pool.
pub struct JobContext {
    /// Unique ID of the job being executed.
    pub job_id: JobId,
    /// Registered name of the job (matches the annotated function name).
    pub name: String,
    /// Queue the job was dispatched from.
    pub queue: String,
    /// Attempt number, starting from 1 on the first execution.
    pub attempt: i32,
    pub(crate) services: ServiceRegistry,
    pub(crate) payload_json: String,
}

impl JobContext {
    /// Deserialize the job payload from JSON.
    ///
    /// The type `T` must match the payload type declared in `#[job]`.
    /// Returns an error if deserialization fails.
    pub fn payload<T: serde::de::DeserializeOwned>(&self) -> Result<T, modo::Error> {
        serde_json::from_str(&self.payload_json)
            .map_err(|e| modo::Error::internal(format!("failed to deserialize job payload: {e}")))
    }

    /// Retrieve a service from the registry by type.
    ///
    /// Returns an error if the service was not registered before the runner started.
    pub fn service<T: Send + Sync + 'static>(&self) -> Result<Arc<T>, modo::Error> {
        self.services.get::<T>().ok_or_else(|| {
            modo::Error::internal(format!(
                "service not registered: {}",
                std::any::type_name::<T>()
            ))
        })
    }

    /// Get the database connection pool.
    ///
    /// Returns an error if the database was not registered via
    /// [`crate::JobsBuilder::service`] before calling `.run()`.
    pub fn db(&self) -> Result<Arc<DbPool>, modo::Error> {
        self.services
            .get::<DbPool>()
            .ok_or_else(|| modo::Error::internal("database not available in job context"))
    }
}

/// Trait for job handlers.
///
/// Implement this trait to define custom job execution logic. In practice you
/// will not implement this directly — the `#[job]` attribute macro generates an
/// implementation automatically.
pub trait JobHandler: Send + Sync + 'static {
    /// Execute the job with the given context.
    fn run(&self, ctx: JobContext) -> impl Future<Output = Result<(), modo::Error>> + Send;
}

/// Object-safe bridge trait for [`JobHandler`].
///
/// Wraps `run` in a pinned boxed future so it can be stored as
/// `Box<dyn JobHandlerDyn>`.  You do not need to implement this manually —
/// there is a blanket implementation for all `JobHandler` types.
pub trait JobHandlerDyn: Send + Sync + 'static {
    /// Execute the job, returning a boxed future.
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
///
/// Created automatically by `#[job]` — do not construct this manually.
pub struct JobRegistration {
    /// Unique job name derived from the annotated function name.
    pub name: &'static str,
    /// Target queue name.
    pub queue: &'static str,
    /// Scheduling priority (higher = runs sooner within the queue).
    pub priority: i32,
    /// Maximum number of execution attempts before the job is marked `dead`.
    pub max_attempts: u32,
    /// Per-execution timeout in seconds.
    pub timeout_secs: u64,
    /// Optional cron expression for scheduled jobs (e.g. `"0 */5 * * * *"`).
    pub cron: Option<&'static str>,
    /// Factory function that creates a new handler instance for each execution.
    pub handler_factory: fn() -> Box<dyn JobHandlerDyn>,
}

inventory::collect!(JobRegistration);
