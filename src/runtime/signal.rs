/// Waits until the process receives a shutdown signal, then returns.
///
/// On **Unix** systems this resolves on either `SIGINT` (Ctrl+C) or `SIGTERM`.
/// On **non-Unix** systems only `SIGINT` / Ctrl+C is handled; the SIGTERM arm
/// becomes a never-resolving future.
///
/// This function is used internally by [`run!`](crate::run) and is also
/// available directly when you need finer-grained control over the shutdown
/// sequence.
///
/// # Panics
///
/// Panics if the OS-level signal handler cannot be installed. This is
/// considered a fatal misconfiguration and is intentional.
pub async fn wait_for_shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
}
