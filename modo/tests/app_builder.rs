use modo::{AppBuilder, ServerConfig};

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
