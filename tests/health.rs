use axum::Router;
use axum::body::Body;
use http::{Request, StatusCode};
use modo::db;
use modo::health::HealthChecks;
use modo::service::Registry;
use tower::ServiceExt;

async fn app_with_db() -> Router {
    let config = db::Config {
        path: ":memory:".into(),
        ..Default::default()
    };
    let database = db::connect(&config).await.unwrap();

    let checks = HealthChecks::new().check("database", database);

    let mut registry = Registry::new();
    registry.add(checks);

    Router::new()
        .merge(modo::health::router())
        .with_state(registry.into_state())
}

#[tokio::test]
async fn live_returns_200_with_real_app() {
    let app = app_with_db().await;
    let resp = app
        .oneshot(Request::get("/_live").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn ready_returns_200_with_database() {
    let app = app_with_db().await;
    let resp = app
        .oneshot(Request::get("/_ready").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_health_failing_check_returns_503() {
    let checks = HealthChecks::new().check_fn("always_down", || async {
        Err(modo::Error::internal("simulated failure"))
    });

    let mut registry = Registry::new();
    registry.add(checks);

    let app = Router::new()
        .merge(modo::health::router())
        .with_state(registry.into_state());

    let resp = app
        .oneshot(Request::get("/_ready").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
}
