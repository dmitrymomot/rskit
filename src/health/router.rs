use axum::Router;
use axum::routing::get;
use http::StatusCode;

use crate::service::{AppState, Service};

use super::HealthChecks;

/// Returns a router with `/_live` and `/_ready` health check endpoints.
///
/// `/_live` always returns 200 OK (liveness probe).
/// `/_ready` extracts [`HealthChecks`] from the registry, runs all checks
/// concurrently, and returns 200 if all pass or 503 if any fail. Failures
/// are logged at ERROR level.
///
/// # Example
///
/// ```
/// use modo::service::Registry;
///
/// let state = Registry::new().into_state();
/// let app: axum::Router = axum::Router::new()
///     .merge(modo::health::router())
///     .with_state(state);
/// ```
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/_live", get(live))
        .route("/_ready", get(ready))
}

async fn live() -> StatusCode {
    StatusCode::OK
}

async fn ready(Service(checks): Service<HealthChecks>) -> StatusCode {
    let entries = checks.as_slice();
    if entries.is_empty() {
        return StatusCode::OK;
    }

    let mut set = tokio::task::JoinSet::new();
    for (name, check) in entries {
        let name = name.clone();
        let check = check.clone();
        set.spawn(async move {
            let result = check.check().await;
            (name, result)
        });
    }

    let mut healthy = true;
    while let Some(join_result) = set.join_next().await {
        match join_result {
            Ok((name, Err(e))) => {
                tracing::error!(
                    check_name = %name,
                    error = %e,
                    "health readiness check failed"
                );
                healthy = false;
            }
            Err(join_err) => {
                tracing::error!(
                    error = %join_err,
                    "health check task panicked"
                );
                healthy = false;
            }
            Ok((_, Ok(()))) => {}
        }
    }

    if healthy {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use http::Request;
    use tower::ServiceExt;

    fn app_with_checks(checks: HealthChecks) -> Router {
        let mut registry = crate::service::Registry::new();
        registry.add(checks);
        router().with_state(registry.into_state())
    }

    #[tokio::test]
    async fn live_returns_200() {
        let checks = HealthChecks::new();
        let app = app_with_checks(checks);
        let resp = app
            .oneshot(Request::get("/_live").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn ready_returns_200_when_all_pass() {
        let checks = HealthChecks::new()
            .check_fn("a", || async { Ok(()) })
            .check_fn("b", || async { Ok(()) });
        let app = app_with_checks(checks);
        let resp = app
            .oneshot(Request::get("/_ready").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn ready_returns_503_when_one_fails() {
        let checks = HealthChecks::new()
            .check_fn("ok", || async { Ok(()) })
            .check_fn("fail", || async { Err(crate::Error::internal("down")) });
        let app = app_with_checks(checks);
        let resp = app
            .oneshot(Request::get("/_ready").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn ready_returns_503_when_all_fail() {
        let checks = HealthChecks::new()
            .check_fn("a", || async { Err(crate::Error::internal("a down")) })
            .check_fn("b", || async { Err(crate::Error::internal("b down")) });
        let app = app_with_checks(checks);
        let resp = app
            .oneshot(Request::get("/_ready").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn ready_returns_200_when_no_checks() {
        let checks = HealthChecks::new();
        let app = app_with_checks(checks);
        let resp = app
            .oneshot(Request::get("/_ready").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn ready_runs_checks_concurrently() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use tokio::sync::Barrier;

        let barrier = Arc::new(Barrier::new(3));
        let counter = Arc::new(AtomicUsize::new(0));

        let b1 = barrier.clone();
        let c1 = counter.clone();
        let b2 = barrier.clone();
        let c2 = counter.clone();
        let b3 = barrier.clone();
        let c3 = counter.clone();

        let checks = HealthChecks::new()
            .check_fn("a", move || {
                let b = b1.clone();
                let c = c1.clone();
                async move {
                    c.fetch_add(1, Ordering::SeqCst);
                    b.wait().await;
                    Ok(())
                }
            })
            .check_fn("b", move || {
                let b = b2.clone();
                let c = c2.clone();
                async move {
                    c.fetch_add(1, Ordering::SeqCst);
                    b.wait().await;
                    Ok(())
                }
            })
            .check_fn("c", move || {
                let b = b3.clone();
                let c = c3.clone();
                async move {
                    c.fetch_add(1, Ordering::SeqCst);
                    b.wait().await;
                    Ok(())
                }
            });

        let app = app_with_checks(checks);
        let resp = app
            .oneshot(Request::get("/_ready").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(counter.load(Ordering::SeqCst), 3);
    }
}
