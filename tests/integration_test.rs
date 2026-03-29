#![cfg(feature = "db")]

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
    let database = db::connect(&config.modo.database).await.unwrap();

    // Registry
    let mut registry = service::Registry::new();
    registry.add(database.clone());

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

    // Verify database works
    let mut rows = database.conn().query("SELECT 1", ()).await.unwrap();
    let row = rows.next().await.unwrap().unwrap();
    let val: i64 = row.get(0).unwrap();
    assert_eq!(val, 1);

    // Verify app_name loaded
    assert_eq!(config.app_name.as_deref(), Some("test-app"));

    // Shutdown
    use modo::runtime::Task;
    handle.shutdown().await.unwrap();
}

#[tokio::test]
#[serial]
async fn test_web_core_bootstrap() {
    unsafe { env::set_var("APP_ENV", "test") };
    let config: TestConfig = config::load("tests/config/").unwrap();
    unsafe { env::remove_var("APP_ENV") };

    let _tracing = modo::tracing::init(&config.modo.tracing).unwrap();
    let database = db::connect(&config.modo.database).await.unwrap();

    let mut registry = service::Registry::new();
    registry.add(database);

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
}
