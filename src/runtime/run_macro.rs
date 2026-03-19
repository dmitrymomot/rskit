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
