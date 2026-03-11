#![cfg(feature = "sse")]

#[test]
fn sse_config_default_values() {
    let config: modo::sse::SseConfig = serde_yaml_ng::from_str("{}").unwrap();
    assert_eq!(config.keep_alive_interval_secs, 15);
}

#[test]
fn sse_config_custom_values() {
    let config: modo::sse::SseConfig =
        serde_yaml_ng::from_str("keep_alive_interval_secs: 30").unwrap();
    assert_eq!(config.keep_alive_interval_secs, 30);
}

#[test]
fn sse_config_keep_alive_interval_method() {
    let config = modo::sse::SseConfig::default();
    assert_eq!(
        config.keep_alive_interval(),
        std::time::Duration::from_secs(15),
    );
}
