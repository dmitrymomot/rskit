use modo::app::AppBuilder;
use modo::config::ServerConfig;

#[test]
fn test_default_server_config() {
    let config = ServerConfig::default();
    assert_eq!(config.port, 3000);
    assert_eq!(config.host, "0.0.0.0");
    assert_eq!(config.bind_address(), "0.0.0.0:3000");
}

#[test]
fn test_app_builder_creates() {
    let builder = AppBuilder::new();
    let _ = builder;
}

#[test]
fn test_app_builder_with_server_config() {
    let config = ServerConfig {
        port: 8080,
        host: "127.0.0.1".to_string(),
        ..Default::default()
    };
    let builder = AppBuilder::new().server_config(config);
    let _ = builder;
}
