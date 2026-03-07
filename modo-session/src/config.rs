use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct SessionConfig {
    pub session_ttl_secs: u64,
    pub cookie_name: String,
    pub validate_fingerprint: bool,
    pub touch_interval_secs: u64,
    pub max_sessions_per_user: usize,
    pub trusted_proxies: Vec<String>,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            session_ttl_secs: 2_592_000, // 30 days
            cookie_name: "_session".to_string(),
            validate_fingerprint: true,
            touch_interval_secs: 300, // 5 minutes
            max_sessions_per_user: 10,
            trusted_proxies: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_values() {
        let config = SessionConfig::default();
        assert_eq!(config.session_ttl_secs, 2_592_000);
        assert_eq!(config.cookie_name, "_session");
        assert!(config.validate_fingerprint);
        assert_eq!(config.touch_interval_secs, 300);
        assert_eq!(config.max_sessions_per_user, 10);
        assert!(config.trusted_proxies.is_empty());
    }

    #[test]
    fn partial_yaml_deserialization() {
        let yaml = r#"
session_ttl_secs: 3600
cookie_name: "my_sess"
"#;
        let config: SessionConfig = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(config.session_ttl_secs, 3600);
        assert_eq!(config.cookie_name, "my_sess");
        // defaults for omitted fields
        assert!(config.validate_fingerprint);
        assert_eq!(config.touch_interval_secs, 300);
        assert_eq!(config.max_sessions_per_user, 10);
    }
}
