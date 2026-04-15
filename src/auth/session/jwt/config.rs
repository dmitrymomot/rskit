use serde::Deserialize;

use super::source::TokenSourceConfig;

/// YAML configuration for JWT session services.
///
/// # Example
///
/// ```yaml
/// jwt:
///   signing_secret: "${JWT_SECRET}"
///   issuer: "my-app"
///   access_ttl_secs: 900
///   refresh_ttl_secs: 2592000
///   max_per_user: 20
///   touch_interval_secs: 300
///   stateful_validation: true
///   access_source:
///     kind: bearer
///   refresh_source:
///     kind: body
///     field: refresh_token
/// ```
#[non_exhaustive]
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct JwtSessionsConfig {
    /// HMAC secret used for signing and verifying tokens.
    pub signing_secret: String,
    /// Required issuer (`iss`). When set, the decoder rejects tokens whose `iss` does not match.
    pub issuer: Option<String>,
    /// Access token lifetime in seconds (default: 900 = 15 minutes).
    pub access_ttl_secs: u64,
    /// Refresh token lifetime in seconds (default: 2592000 = 30 days).
    pub refresh_ttl_secs: u64,
    /// Maximum concurrent sessions per user (default: 20).
    pub max_per_user: usize,
    /// Minimum interval between session touch updates in seconds (default: 300).
    pub touch_interval_secs: u64,
    /// When `true`, tokens are validated against the session store on every request.
    pub stateful_validation: bool,
    /// Where to extract access tokens from incoming requests.
    pub access_source: TokenSourceConfig,
    /// Where to extract refresh tokens from incoming requests.
    pub refresh_source: TokenSourceConfig,
}

impl Default for JwtSessionsConfig {
    fn default() -> Self {
        Self {
            signing_secret: String::new(),
            issuer: None,
            access_ttl_secs: 900,
            refresh_ttl_secs: 2_592_000,
            max_per_user: 20,
            touch_interval_secs: 300,
            stateful_validation: true,
            access_source: TokenSourceConfig::Bearer,
            refresh_source: TokenSourceConfig::Body {
                field: "refresh_token".into(),
            },
        }
    }
}

impl JwtSessionsConfig {
    /// Create a JWT sessions configuration with the given HMAC signing secret.
    pub fn new(signing_secret: impl Into<String>) -> Self {
        Self {
            signing_secret: signing_secret.into(),
            ..Self::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_full_config() {
        let yaml = r#"
            signing_secret: "my-secret"
            issuer: "my-app"
            access_ttl_secs: 900
            refresh_ttl_secs: 2592000
            max_per_user: 20
            touch_interval_secs: 300
            stateful_validation: true
        "#;
        let config: JwtSessionsConfig = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(config.signing_secret, "my-secret");
        assert_eq!(config.issuer.as_deref(), Some("my-app"));
        assert_eq!(config.access_ttl_secs, 900);
        assert_eq!(config.refresh_ttl_secs, 2_592_000);
        assert_eq!(config.max_per_user, 20);
        assert_eq!(config.touch_interval_secs, 300);
        assert!(config.stateful_validation);
    }

    #[test]
    fn deserialize_minimal_config() {
        let yaml = r#"signing_secret: "my-secret""#;
        let config: JwtSessionsConfig = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(config.signing_secret, "my-secret");
        assert!(config.issuer.is_none());
        assert_eq!(config.access_ttl_secs, 900);
    }

    #[test]
    fn missing_secret_defaults_to_empty() {
        let yaml = r#"access_ttl_secs: 1800"#;
        let config: JwtSessionsConfig = serde_yaml_ng::from_str(yaml).unwrap();
        assert!(config.signing_secret.is_empty());
        assert_eq!(config.access_ttl_secs, 1800);
    }

    #[test]
    fn default_values() {
        let config = JwtSessionsConfig::default();
        assert!(config.signing_secret.is_empty());
        assert!(config.issuer.is_none());
        assert_eq!(config.access_ttl_secs, 900);
        assert_eq!(config.refresh_ttl_secs, 2_592_000);
        assert_eq!(config.max_per_user, 20);
        assert_eq!(config.touch_interval_secs, 300);
        assert!(config.stateful_validation);
    }

    #[test]
    fn new_sets_signing_secret() {
        let config = JwtSessionsConfig::new("my-super-secret");
        assert_eq!(config.signing_secret, "my-super-secret");
        assert_eq!(config.access_ttl_secs, 900);
    }
}
