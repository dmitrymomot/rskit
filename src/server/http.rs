use super::Config;
use crate::error::Result;
use crate::runtime::Task;

pub struct HttpServer {
    shutdown_tx: tokio::sync::oneshot::Sender<()>,
    handle: tokio::task::JoinHandle<()>,
}

impl Task for HttpServer {
    async fn shutdown(self) -> Result<()> {
        let _ = self.shutdown_tx.send(());
        let _ = self.handle.await;
        Ok(())
    }
}

pub async fn http(router: axum::Router, config: &Config) -> Result<HttpServer> {
    let addr = format!("{}:{}", config.host, config.port);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .map_err(|e| crate::error::Error::internal(format!("failed to bind to {addr}: {e}")))?;

    let local_addr = listener
        .local_addr()
        .map_err(|e| crate::error::Error::internal(format!("failed to get local address: {e}")))?;

    tracing::info!("server listening on {local_addr}");

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    let handle = tokio::spawn(async move {
        axum::serve(listener, router.into_make_service())
            .with_graceful_shutdown(async {
                let _ = shutdown_rx.await;
            })
            .await
            .ok();
    });

    Ok(HttpServer {
        shutdown_tx,
        handle,
    })
}
