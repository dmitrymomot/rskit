/// Waits for a shutdown signal and then shuts down each task in order.
///
/// `run!` accepts one or more expressions that implement [`Task`](crate::runtime::Task).
/// It returns an `async` block that, when `.await`ed:
///
/// 1. Calls [`wait_for_shutdown_signal`](crate::runtime::wait_for_shutdown_signal)
///    and blocks until `SIGINT` or `SIGTERM` is received.
/// 2. Iterates through each supplied task **in declaration order**, calling
///    [`Task::shutdown`](crate::runtime::Task::shutdown) on each one.
/// 3. Logs a tracing `info` event for each step and an `error` event for any
///    task that returns `Err`.
/// 4. Returns `Ok::<(), modo::Error>(())`.
///
/// # Example
///
/// ```rust,no_run
/// use modo::runtime::Task;
/// use modo::Result;
///
/// struct Worker;
/// struct HttpServer;
///
/// impl Task for Worker {
///     async fn shutdown(self) -> Result<()> { Ok(()) }
/// }
///
/// impl Task for HttpServer {
///     async fn shutdown(self) -> Result<()> { Ok(()) }
/// }
///
/// #[tokio::main]
/// async fn main() -> Result<()> {
///     let worker = Worker;
///     let server = HttpServer;
///     modo::run!(worker, server).await
/// }
/// ```
#[macro_export]
macro_rules! run {
    ($($task:expr),+ $(,)?) => {
        async {
            $crate::runtime::wait_for_shutdown_signal().await;
            $crate::tracing::info!("shutdown signal received, stopping services...");

            $(
                let task_name = stringify!($task);
                $crate::tracing::info!(task = task_name, "shutting down");
                if let Err(e) = $crate::runtime::Task::shutdown($task).await {
                    $crate::tracing::error!(task = task_name, error = %e, "shutdown error");
                }
            )+

            $crate::tracing::info!("all services stopped");
            Ok::<(), $crate::error::Error>(())
        }
    };
}
