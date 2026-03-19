use serial_test::serial;

#[test]
#[serial]
fn test_tracing_config_defaults() {
    let config = modo::tracing::Config::default();
    assert_eq!(config.level, "info");
    assert_eq!(config.format, "pretty");
}

#[test]
#[serial]
fn test_tracing_init_succeeds() {
    let config = modo::tracing::Config::default();
    let result = modo::tracing::init(&config);
    assert!(result.is_ok());
}

#[test]
#[serial]
fn test_tracing_init_idempotent() {
    let config = modo::tracing::Config::default();
    let r1 = modo::tracing::init(&config);
    let r2 = modo::tracing::init(&config);
    assert!(r1.is_ok());
    assert!(r2.is_ok());
}

#[test]
#[serial]
fn test_tracing_init_json_format() {
    let config = modo::tracing::Config {
        format: "json".to_string(),
        ..Default::default()
    };
    let result = modo::tracing::init(&config);
    assert!(result.is_ok());
}

#[test]
#[serial]
fn test_tracing_init_unknown_format_fallback() {
    let config = modo::tracing::Config {
        format: "unknown".to_string(),
        ..Default::default()
    };
    let result = modo::tracing::init(&config);
    assert!(result.is_ok());
}
