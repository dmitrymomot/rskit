use axum::http::StatusCode;
use axum::response::IntoResponse;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// An async function used to check application readiness.
///
/// Returns `Ok(())` when the check passes and `Err(_)` to signal that the
/// application is not yet ready to serve traffic. Register checks via
/// `AppBuilder::readiness_check`.
pub type ReadinessCheck = Arc<
    dyn Fn() -> Pin<
            Box<dyn Future<Output = Result<(), Box<dyn std::error::Error + Send + Sync>>> + Send>,
        > + Send
        + Sync,
>;

/// Handler for the liveness probe (`/_live`). Always returns `200 OK`.
pub async fn liveness_handler() -> impl IntoResponse {
    (StatusCode::OK, "ok")
}

/// Handler for the readiness probe (`/_ready`).
///
/// Runs all registered checks sequentially. Returns `200 OK` when all pass,
/// or `503 Service Unavailable` on the first failure.
pub async fn readiness_handler(checks: Vec<ReadinessCheck>) -> impl IntoResponse {
    for check in &checks {
        if let Err(e) = check().await {
            tracing::error!(error = %e, "Readiness check failed");
            return (StatusCode::SERVICE_UNAVAILABLE, "service unavailable").into_response();
        }
    }
    (StatusCode::OK, "ok").into_response()
}
