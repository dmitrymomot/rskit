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
    ///
    /// # Errors
    ///
    /// Returns [`modo::Error`](crate::Error) only when teardown encounters a
    /// genuinely unexpected failure (for example, a worker panic surfaced via a
    /// join handle). Normal, clean shutdown — including cases where a resource
    /// is already closed — should return `Ok(())`. An error from one task does
    /// not abort the remaining shutdown sequence in [`run!`](crate::run); it is
    /// logged at `error` level and the next task is still invoked.
    fn shutdown(self) -> impl std::future::Future<Output = Result<()>> + Send;
}
