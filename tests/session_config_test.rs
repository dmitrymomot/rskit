use modo::auth::session::SessionConfig;

#[test]
fn test_default_values() {
    let config = SessionConfig::default();
    assert_eq!(config.session_ttl_secs, 2_592_000);
    assert_eq!(config.cookie_name, "_session");
    assert!(config.validate_fingerprint);
    assert_eq!(config.touch_interval_secs, 300);
    assert_eq!(config.max_sessions_per_user, 10);
}

#[test]
fn test_partial_yaml_deserialization() {
    let yaml = r#"
session_ttl_secs: 3600
cookie_name: "my_sess"
"#;
    let config: SessionConfig = serde_yaml_ng::from_str(yaml).unwrap();
    assert_eq!(config.session_ttl_secs, 3600);
    assert_eq!(config.cookie_name, "my_sess");
    assert!(config.validate_fingerprint);
    assert_eq!(config.touch_interval_secs, 300);
    assert_eq!(config.max_sessions_per_user, 10);
}

#[test]
fn test_zero_max_sessions_returns_error() {
    let yaml = r#"
max_sessions_per_user: 0
"#;
    let err = serde_yaml_ng::from_str::<SessionConfig>(yaml).unwrap_err();
    assert!(
        err.to_string()
            .contains("max_sessions_per_user must be > 0"),
        "unexpected error: {err}",
    );
}

#[test]
fn test_nonzero_max_sessions_accepted() {
    let yaml = r#"
max_sessions_per_user: 1
"#;
    let config: SessionConfig = serde_yaml_ng::from_str(yaml).unwrap();
    assert_eq!(config.max_sessions_per_user, 1);
}
