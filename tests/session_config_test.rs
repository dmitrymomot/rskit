use modo::auth::session::CookieSessionsConfig;

#[test]
fn test_default_values() {
    let config = CookieSessionsConfig::default();
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
    let config: CookieSessionsConfig = serde_yaml_ng::from_str(yaml).unwrap();
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
    let err = serde_yaml_ng::from_str::<CookieSessionsConfig>(yaml).unwrap_err();
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
    let config: CookieSessionsConfig = serde_yaml_ng::from_str(yaml).unwrap();
    assert_eq!(config.max_sessions_per_user, 1);
}

#[test]
fn test_nested_cookie_field_defaults() {
    let config = CookieSessionsConfig::default();
    assert!(config.cookie.secure);
    assert!(config.cookie.http_only);
    assert_eq!(config.cookie.same_site, "lax");
}

#[test]
fn test_nested_cookie_yaml_deserialization() {
    let yaml = r#"
session_ttl_secs: 1800
cookie:
  secret: "supersecretkey64charslong_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
  secure: false
  same_site: "strict"
"#;
    let config: CookieSessionsConfig = serde_yaml_ng::from_str(yaml).unwrap();
    assert_eq!(config.session_ttl_secs, 1800);
    assert!(!config.cookie.secure);
    assert_eq!(config.cookie.same_site, "strict");
    assert!(config.cookie.http_only); // default preserved
}
