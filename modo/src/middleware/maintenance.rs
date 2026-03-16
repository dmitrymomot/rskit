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
    // Bypass health check paths — strip trailing slash from both sides so
    // `/_live/` matches `/_live` regardless of config.
    let path = request.uri().path();
    let normalized = path.strip_suffix('/').unwrap_or(path);
    let liveness = state
        .server_config
        .liveness_path
        .strip_suffix('/')
        .unwrap_or(&state.server_config.liveness_path);
    let readiness = state
        .server_config
        .readiness_path
        .strip_suffix('/')
        .unwrap_or(&state.server_config.readiness_path);
    if normalized == liveness || normalized == readiness {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{AppState, ServiceRegistry};
    use axum::Router;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::routing::get;
    use tower::ServiceExt;

    fn test_state(maintenance: bool) -> AppState {
        let mut server_config = crate::config::ServerConfig::default();
        server_config.http.maintenance = maintenance;
        AppState {
            services: ServiceRegistry::new(),
            server_config,
            cookie_key: axum_extra::extract::cookie::Key::generate(),
        }
    }

    #[tokio::test]
    async fn health_path_bypasses_maintenance() {
        let state = test_state(true);
        let app = Router::new()
            .route("/_live", get(|| async { "ok" }))
            .layer(axum::middleware::from_fn_with_state(
                state.clone(),
                maintenance_middleware,
            ))
            .with_state(state);

        let resp = app
            .oneshot(Request::get("/_live").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn health_path_with_trailing_slash_bypasses_maintenance() {
        let state = test_state(true);
        let app = Router::new()
            .route("/_live", get(|| async { "ok" }))
            .route("/_live/", get(|| async { "ok" }))
            .layer(axum::middleware::from_fn_with_state(
                state.clone(),
                maintenance_middleware,
            ))
            .with_state(state);

        let resp = app
            .oneshot(Request::get("/_live/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        // Should bypass maintenance — trailing slash should match /_live
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn non_health_path_blocked_by_maintenance() {
        let state = test_state(true);
        let app = Router::new()
            .route("/api/data", get(|| async { "data" }))
            .layer(axum::middleware::from_fn_with_state(
                state.clone(),
                maintenance_middleware,
            ))
            .with_state(state);

        let resp = app
            .oneshot(Request::get("/api/data").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }
}
