use std::future::Future;
use std::pin::Pin;

/// Determines when a managed service is shut down during graceful shutdown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ShutdownPhase {
    /// Drain active work (jobs, cron). Runs before user hooks.
    Drain = 0,
    /// Close infrastructure (DB pools, caches). Runs after user hooks.
    Close = 1,
}

/// Trait for services that participate in graceful shutdown.
///
/// Register via `app.managed_service(svc)` to have `graceful_shutdown()`
/// called automatically in the correct phase when the server stops.
pub trait GracefulShutdown: Send + Sync + 'static {
    /// Perform shutdown work (drain queues, close connections, etc.).
    fn graceful_shutdown(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>>;

    /// Which phase this service shuts down in. Defaults to `Close`.
    fn shutdown_phase(&self) -> ShutdownPhase {
        ShutdownPhase::Close
    }
}
