use crate::error::Error;
use crate::jobs::types::JobContext;
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

/// Trait for job handler execution.
pub trait JobHandler: Send + Sync + 'static {
    fn run(&self, ctx: JobContext) -> impl Future<Output = Result<(), Error>> + Send;
}

/// Object-safe version of `JobHandler`.
pub trait JobHandlerDyn: Send + Sync + 'static {
    fn run<'a>(
        &'a self,
        ctx: JobContext,
    ) -> Pin<Box<dyn Future<Output = Result<(), Error>> + Send + 'a>>;
}

impl<T: JobHandler> JobHandlerDyn for T {
    fn run<'a>(
        &'a self,
        ctx: JobContext,
    ) -> Pin<Box<dyn Future<Output = Result<(), Error>> + Send + 'a>> {
        Box::pin(JobHandler::run(self, ctx))
    }
}

/// Registration entry for a job handler, collected via `inventory`.
pub struct JobRegistration {
    pub name: &'static str,
    pub queue: &'static str,
    pub max_retries: u32,
    pub timeout: Duration,
    pub cron: Option<&'static str>,
    pub handler_factory: fn() -> Box<dyn JobHandlerDyn>,
}

inventory::collect!(JobRegistration);
