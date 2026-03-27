use crate::error::Result;

/// A service or resource that can be shut down gracefully.
///
/// Implement this trait for every long-running component that needs cleanup on
/// process exit — HTTP servers, background workers, connection pools, etc.
/// The [`run!`](crate::run) macro calls [`Task::shutdown`] on each registered
/// task in declaration order after a shutdown signal is received.
///
/// # Contract
///
/// - Implementors must be `Send + 'static` so they can be moved across threads.
/// - `shutdown` consumes `self` — the task cannot be used after shutdown.
/// - Return `Err` only for genuinely unexpected failures; normal teardown should
///   return `Ok(())`.
pub trait Task: Send + 'static {
    /// Shuts down this task and releases its resources.
    ///
    /// Called once by [`run!`](crate::run) after the process receives a
    /// shutdown signal. The future must be `Send` because it may be awaited on
    /// any Tokio thread.
    fn shutdown(self) -> impl std::future::Future<Output = Result<()>> + Send;
}
