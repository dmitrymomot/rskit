use std::time::Duration;

use tower::Layer;
use tower_http::normalize_path::NormalizePathLayer;

use super::Config;
use crate::error::Result;
use crate::runtime::Task;

/// An opaque handle to the running HTTP server.
///
/// Implements [`Task`] so it can be passed to the [`crate::run!`] macro for
/// coordinated graceful shutdown alongside other services.
///
/// Obtain a handle by calling [`http()`].
pub struct HttpServer {
    shutdown_tx: tokio::sync::oneshot::Sender<()>,
    handle: tokio::task::JoinHandle<()>,
    shutdown_timeout: Duration,
}

impl Task for HttpServer {
    /// Signal the server to stop accepting new connections and wait for
    /// in-flight requests to drain.
    ///
    /// If the drain does not complete within `shutdown_timeout_secs` (from
    /// [`Config`]), a warning is logged and the function still returns
    /// `Ok(())` — shutdown is best-effort.
    ///
    /// # Errors
    ///
    /// This implementation is infallible and always returns `Ok(())`; the
    /// [`Result`] signature comes from the [`Task`] trait.
    async fn shutdown(self) -> Result<()> {
        let _ = self.shutdown_tx.send(());
        if tokio::time::timeout(self.shutdown_timeout, self.handle)
            .await
            .is_err()
        {
            tracing::warn!("server shutdown timed out, forcing exit");
        }
        Ok(())
    }
}

/// Bind a TCP listener and start serving `router`.
///
/// `router` may be any value that implements `Into<axum::Router>`, including
/// [`axum::Router`] itself and [`HostRouter`](super::HostRouter). The router
/// is wrapped in a `NormalizePathLayer` so trailing slashes are trimmed before
/// path matching (root `/` is preserved).
///
/// Returns an [`HttpServer`] handle immediately; the server runs on a
/// background Tokio task. Pass the handle to [`crate::run!`] so it is
/// shut down gracefully when a signal arrives.
///
/// # Errors
///
/// Returns [`crate::Error::internal`] if the TCP port cannot be bound
/// (address already in use, permission denied, malformed host) or if the
/// bound socket's local address cannot be read.
///
/// # Example
///
/// ```rust,no_run
/// use modo::server::{Config, http};
///
/// #[tokio::main]
/// async fn main() -> modo::Result<()> {
///     let config = Config::default();
///     let router = modo::axum::Router::new();
///     let server = http(router, &config).await?;
///     modo::run!(server).await
/// }
/// ```
///
/// With a [`HostRouter`](super::HostRouter):
///
/// ```rust,no_run
/// use modo::server::{self, Config, HostRouter};
///
/// #[tokio::main]
/// async fn main() -> modo::Result<()> {
///     let config = Config::default();
///     let app = HostRouter::new()
///         .host("acme.com", modo::axum::Router::new())
///         .host("*.acme.com", modo::axum::Router::new());
///     let server = server::http(app, &config).await?;
///     modo::run!(server).await
/// }
/// ```
pub async fn http(router: impl Into<axum::Router>, config: &Config) -> Result<HttpServer> {
    let router = router.into();
    let addr = format!("{}:{}", config.host, config.port);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .map_err(|e| crate::error::Error::internal(format!("failed to bind to {addr}: {e}")))?;

    let local_addr = listener
        .local_addr()
        .map_err(|e| crate::error::Error::internal(format!("failed to get local address: {e}")))?;

    tracing::info!("server listening on {local_addr}");

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    // Wrap the router with NormalizePathLayer so trailing slashes are stripped
    // before axum performs path matching. Applied here (not via Router::layer)
    // because Router::layer runs *after* path matching.
    let service = NormalizePathLayer::trim_trailing_slash().layer(router);
    let make_service = axum::ServiceExt::<axum::extract::Request>::into_make_service(service);

    let handle = tokio::spawn(async move {
        axum::serve(listener, make_service)
            .with_graceful_shutdown(async {
                let _ = shutdown_rx.await;
            })
            .await
            .ok();
    });

    Ok(HttpServer {
        shutdown_tx,
        handle,
        shutdown_timeout: Duration::from_secs(config.shutdown_timeout_secs),
    })
}
