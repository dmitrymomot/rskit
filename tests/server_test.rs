#[test]
fn test_server_config_defaults() {
    let config = modo::server::Config::default();
    assert_eq!(config.host, "localhost");
    assert_eq!(config.port, 8080);
    assert_eq!(config.shutdown_timeout_secs, 30);
}

#[tokio::test]
async fn test_server_starts_and_stops() {
    use modo::runtime::Task;
    use modo::service::{AppState, Registry};

    let config = modo::server::Config {
        host: "127.0.0.1".to_string(),
        port: 0,
        shutdown_timeout_secs: 5,
    };

    let state: AppState = Registry::new().into_state();

    let router = axum::Router::new()
        .route("/health", axum::routing::get(|| async { "ok" }))
        .with_state(state);

    let handle = modo::server::http(router, &config).await.unwrap();
    handle.shutdown().await.unwrap();
}

#[tokio::test]
async fn test_server_binding_failure() {
    let config = modo::server::Config {
        host: "999.999.999.999".to_string(),
        port: 0,
        shutdown_timeout_secs: 5,
    };

    let router = axum::Router::new();
    let result = modo::server::http(router, &config).await;
    assert!(result.is_err());
    let err = result.err().unwrap();
    assert!(
        err.message().contains("failed to bind"),
        "expected 'failed to bind' in error message, got: {}",
        err.message()
    );
}

#[test]
fn test_server_config_deserialize_partial() {
    let config: modo::server::Config = serde_json::from_str(r#"{"port": 3000}"#).unwrap();
    assert_eq!(config.host, "localhost");
    assert_eq!(config.port, 3000);
    assert_eq!(config.shutdown_timeout_secs, 30);
}

#[test]
fn test_server_config_custom_values() {
    let config = modo::server::Config {
        host: "0.0.0.0".to_string(),
        port: 9090,
        shutdown_timeout_secs: 60,
    };
    assert_eq!(config.host, "0.0.0.0");
    assert_eq!(config.port, 9090);
    assert_eq!(config.shutdown_timeout_secs, 60);
}
