use axum::{Json, Router, routing::get};
use modo::{config, db, server, service};
use serde::Deserialize;
use serial_test::serial;
use std::env;

#[derive(Deserialize)]
struct TestConfig {
    #[serde(flatten)]
    modo: modo::Config,
    app_name: Option<String>,
}

#[tokio::test]
#[serial]
async fn test_full_bootstrap() {
    // Setup
    unsafe { env::set_var("APP_ENV", "test") };
    let config: TestConfig = config::load("tests/config/").unwrap();
    unsafe { env::remove_var("APP_ENV") };

    // Tracing
    let _tracing = modo::tracing::init(&config.modo.tracing).unwrap();

    // Database
    let pool = db::connect(&config.modo.database).await.unwrap();

    // Registry
    let mut registry = service::Registry::new();
    registry.add(pool.clone());

    // Router
    let state = registry.into_state();
    let router = Router::new()
        .route(
            "/health",
            get(|| async { Json(serde_json::json!({"status": "ok"})) }),
        )
        .with_state(state);

    // Server
    let handle = server::http(router, &config.modo.server).await.unwrap();

    // Verify pool works
    let row: (i64,) = sqlx::query_as("SELECT 1").fetch_one(&*pool).await.unwrap();
    assert_eq!(row.0, 1);

    // Verify app_name loaded
    assert_eq!(config.app_name.as_deref(), Some("test-app"));

    // Shutdown
    use modo::runtime::Task;
    handle.shutdown().await.unwrap();
    pool.close().await;
}

#[tokio::test]
#[serial]
async fn test_web_core_bootstrap() {
    unsafe { env::set_var("APP_ENV", "test") };
    let config: TestConfig = config::load("tests/config/").unwrap();
    unsafe { env::remove_var("APP_ENV") };

    let _tracing = modo::tracing::init(&config.modo.tracing).unwrap();
    let pool = db::connect(&config.modo.database).await.unwrap();

    let mut registry = service::Registry::new();
    registry.add(pool.clone());

    let state = registry.into_state();
    let router = Router::new()
        .route("/health", get(|| async { "ok" }))
        .layer(modo::middleware::compression())
        .layer(modo::middleware::request_id())
        .with_state(state);

    let handle = server::http(router, &config.modo.server).await.unwrap();

    use modo::runtime::Task;
    handle.shutdown().await.unwrap();
    _tracing.shutdown().await.unwrap();
    pool.close().await;
}
