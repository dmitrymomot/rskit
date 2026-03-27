use axum::Router;
use axum::body::Body;
use http::{Request, StatusCode};
use modo::db::{Pool, ReadPool, WritePool};
use modo::health::HealthChecks;
use modo::service::Registry;
use tower::ServiceExt;

fn app_with_real_pools(pool: Pool) -> Router {
    let read = ReadPool::new((*pool).clone());
    let write = WritePool::new((*pool).clone());

    let checks = HealthChecks::new()
        .check("read_pool", read)
        .check("write_pool", write)
        .check("pool", pool);

    let mut registry = Registry::new();
    registry.add(checks);

    Router::new()
        .merge(modo::health::router())
        .with_state(registry.into_state())
}

#[tokio::test]
async fn live_returns_200_with_real_app() {
    let pool = Pool::new(sqlx::SqlitePool::connect(":memory:").await.unwrap());
    let app = app_with_real_pools(pool);
    let resp = app
        .oneshot(Request::get("/_live").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn ready_returns_200_with_real_pools() {
    let pool = Pool::new(sqlx::SqlitePool::connect(":memory:").await.unwrap());
    let app = app_with_real_pools(pool);
    let resp = app
        .oneshot(Request::get("/_ready").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}
