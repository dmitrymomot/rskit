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
    pub secret: String,
    pub default_expiry: Option<u64>,
    #[serde(default)]
    pub leeway: u64,
    pub issuer: Option<String>,
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
