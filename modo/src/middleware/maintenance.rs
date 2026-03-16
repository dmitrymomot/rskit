use crate::app::AppState;
use crate::error::HttpError;
use axum::extract::State;
use axum::http::Request;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

pub async fn maintenance_middleware(
    State(state): State<AppState>,
    request: Request<axum::body::Body>,
    next: Next,
) -> Response {
    // Bypass health check paths
    let path = request.uri().path();
    if path == state.server_config.liveness_path || path == state.server_config.readiness_path {
        return next.run(request).await;
    }

    let msg = state
        .server_config
        .http
        .maintenance_message
        .as_deref()
        .unwrap_or("service temporarily unavailable");

    HttpError::ServiceUnavailable
        .with_message(msg)
        .into_response()
}
