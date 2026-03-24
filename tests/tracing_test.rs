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
    let config = {
        let mut c = modo::tracing::Config::default();
        c.format = "json".to_string();
        c
    };
    let result = modo::tracing::init(&config);
    assert!(result.is_ok());
}

#[test]
#[serial]
fn test_tracing_init_unknown_format_fallback() {
    let config = {
        let mut c = modo::tracing::Config::default();
        c.format = "unknown".to_string();
        c
    };
    let result = modo::tracing::init(&config);
    assert!(result.is_ok());
}

#[tokio::test]
#[serial]
async fn test_tracing_init_returns_guard() {
    let config = modo::tracing::Config::default();
    let guard = modo::tracing::init(&config);
    assert!(guard.is_ok());

    use modo::runtime::Task;
    guard.unwrap().shutdown().await.unwrap();
}
