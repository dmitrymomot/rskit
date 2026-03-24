#![cfg(feature = "auth")]

use modo::auth::oauth::{CallbackParams, OAuthConfig, OAuthProviderConfig};

#[test]
fn oauth_config_default_is_empty() {
    let config = OAuthConfig::default();
    assert!(config.google.is_none());
    assert!(config.github.is_none());
}

#[test]
fn provider_config_deserializes() {
    let yaml = r#"
client_id: "test-id"
client_secret: "test-secret"
redirect_uri: "http://localhost:8080/callback"
"#;
    let config: OAuthProviderConfig = serde_yaml_ng::from_str(yaml).unwrap();
    assert_eq!(config.client_id, "test-id");
    assert_eq!(config.client_secret, "test-secret");
    assert_eq!(config.redirect_uri, "http://localhost:8080/callback");
    assert!(config.scopes.is_empty());
}

#[test]
fn provider_config_with_scopes() {
    let yaml = r#"
client_id: "test-id"
client_secret: "test-secret"
redirect_uri: "http://localhost:8080/callback"
scopes:
  - "openid"
  - "email"
"#;
    let config: OAuthProviderConfig = serde_yaml_ng::from_str(yaml).unwrap();
    assert_eq!(config.scopes, vec!["openid", "email"]);
}

#[test]
fn oauth_config_partial() {
    let yaml = r#"
google:
  client_id: "gid"
  client_secret: "gsecret"
  redirect_uri: "http://localhost/google"
"#;
    let config: OAuthConfig = serde_yaml_ng::from_str(yaml).unwrap();
    assert!(config.google.is_some());
    assert!(config.github.is_none());
}

#[test]
fn callback_params_deserializes() {
    let json = r#"{"code":"abc123","state":"xyz789"}"#;
    let params: CallbackParams = serde_json::from_str(json).unwrap();
    assert_eq!(params.code, "abc123");
    assert_eq!(params.state, "xyz789");
}

#[test]
fn user_profile_serializes() {
    let profile = {
        let mut p = modo::auth::oauth::UserProfile::new("google", "123", "user@example.com");
        p.email_verified = true;
        p.name = Some("Test User".to_string());
        p.raw = serde_json::json!({"locale": "en"});
        p
    };
    let json = serde_json::to_string(&profile).unwrap();
    assert!(json.contains("\"email_verified\":true"));
    assert!(json.contains("\"provider\":\"google\""));
}
