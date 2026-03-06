use modo::app::AppBuilder;
use modo::config::AppConfig;

#[test]
fn test_default_config() {
    let config = AppConfig::default();
    assert_eq!(config.bind_address, "0.0.0.0:3000");
    assert_eq!(config.database_url, "sqlite://data.db?mode=rwc");
}

#[test]
fn test_app_builder_creates() {
    let config = AppConfig::default();
    let builder = AppBuilder::new(config);
    let _ = builder;
}
