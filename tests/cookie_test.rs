#[test]
fn test_cookie_config_deserialize() {
    let yaml = r#"
secret: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
secure: false
http_only: true
same_site: strict
path: /app
"#;
    let config: modo::cookie::CookieConfig = serde_yaml_ng::from_str(yaml).unwrap();
    assert_eq!(config.secret.len(), 64);
    assert!(!config.secure);
    assert!(config.http_only);
    assert_eq!(config.same_site, "strict");
    assert_eq!(config.path, "/app");
}

#[test]
fn test_cookie_config_requires_secret() {
    let yaml = r#"
secure: true
"#;
    let result = serde_yaml_ng::from_str::<modo::cookie::CookieConfig>(yaml);
    assert!(result.is_err());
}

#[test]
fn test_cookie_config_defaults() {
    let yaml = r#"
secret: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
"#;
    let config: modo::cookie::CookieConfig = serde_yaml_ng::from_str(yaml).unwrap();
    assert!(config.secure);
    assert!(config.http_only);
    assert_eq!(config.same_site, "lax");
    assert_eq!(config.path, "/");
    assert!(config.domain.is_none());
}

#[test]
fn test_key_from_config_success() {
    let config = modo::cookie::CookieConfig {
        secret: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_string(),
        secure: true,
        http_only: true,
        same_site: "lax".to_string(),
        path: "/".to_string(),
        domain: None,
    };
    let key = modo::cookie::key_from_config(&config);
    assert!(key.is_ok());
}

#[test]
fn test_key_from_config_too_short() {
    let config = modo::cookie::CookieConfig {
        secret: "tooshort".to_string(),
        secure: true,
        http_only: true,
        same_site: "lax".to_string(),
        path: "/".to_string(),
        domain: None,
    };
    let key = modo::cookie::key_from_config(&config);
    assert!(key.is_err());
}

#[test]
fn test_cookie_config_with_domain() {
    let yaml = r#"
secret: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
domain: "example.com"
"#;
    let config: modo::cookie::CookieConfig = serde_yaml_ng::from_str(yaml).unwrap();
    assert_eq!(config.domain, Some("example.com".to_string()));
    assert!(config.secure);
    assert!(config.http_only);
    assert_eq!(config.same_site, "lax");
    assert_eq!(config.path, "/");
}
