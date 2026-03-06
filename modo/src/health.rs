use axum::http::StatusCode;
use axum::response::IntoResponse;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

pub type ReadinessCheck = Arc<
    dyn Fn() -> Pin<
            Box<dyn Future<Output = Result<(), Box<dyn std::error::Error + Send + Sync>>> + Send>,
        > + Send
        + Sync,
>;

pub async fn liveness_handler() -> impl IntoResponse {
    (StatusCode::OK, "ok")
}

pub async fn readiness_handler(checks: Vec<ReadinessCheck>) -> impl IntoResponse {
    for check in &checks {
        if let Err(e) = check().await {
            tracing::error!(error = %e, "Readiness check failed");
            return (StatusCode::INTERNAL_SERVER_ERROR, "internal server error").into_response();
        }
    }
    (StatusCode::OK, "ok").into_response()
}
