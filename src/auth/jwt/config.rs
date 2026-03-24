use serde::Deserialize;

/// YAML configuration for JWT services.
///
/// # Example
///
/// ```yaml
/// jwt:
///   secret: "${JWT_SECRET}"
///   default_expiry: 3600
///   leeway: 5
///   issuer: "my-app"
///   audience: "api"
/// ```
#[derive(Debug, Deserialize)]
pub struct JwtConfig {
    /// HMAC secret used for signing and verifying tokens.
    pub secret: String,
    /// Default token lifetime in seconds. Applied automatically by `JwtEncoder::encode()`
    /// when `claims.exp` is `None`. If `None`, tokens without an `exp` are rejected by the decoder.
    pub default_expiry: Option<u64>,
    /// Clock skew tolerance in seconds. Applied to both `exp` and `nbf` checks.
    /// Defaults to `0` when omitted from YAML.
    #[serde(default)]
    pub leeway: u64,
    /// Required issuer (`iss`). When set, `JwtDecoder::decode()` rejects tokens
    /// whose `iss` does not match.
    pub issuer: Option<String>,
    /// Required audience (`aud`). When set, `JwtDecoder::decode()` rejects tokens
    /// whose `aud` does not match.
    pub audience: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_full_config() {
        let yaml = r#"
            secret: "my-secret"
            default_expiry: 3600
            leeway: 5
            issuer: "my-app"
            audience: "api"
        "#;
        let config: JwtConfig = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(config.secret, "my-secret");
        assert_eq!(config.default_expiry, Some(3600));
        assert_eq!(config.leeway, 5);
        assert_eq!(config.issuer.as_deref(), Some("my-app"));
        assert_eq!(config.audience.as_deref(), Some("api"));
    }

    #[test]
    fn deserialize_minimal_config() {
        let yaml = r#"secret: "my-secret""#;
        let config: JwtConfig = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(config.secret, "my-secret");
        assert!(config.default_expiry.is_none());
        assert_eq!(config.leeway, 0);
        assert!(config.issuer.is_none());
        assert!(config.audience.is_none());
    }

    #[test]
    fn missing_secret_fails() {
        let yaml = r#"leeway: 5"#;
        let result = serde_yaml_ng::from_str::<JwtConfig>(yaml);
        assert!(result.is_err());
    }
}
