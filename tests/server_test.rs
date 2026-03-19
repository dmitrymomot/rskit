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
