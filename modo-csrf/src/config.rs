use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct CsrfConfig {
    pub cookie_name: String,
    pub field_name: String,
    pub header_name: String,
    pub cookie_max_age: u64,
    pub token_length: usize,
    pub secure: bool,
}

impl Default for CsrfConfig {
    fn default() -> Self {
        Self {
            cookie_name: "_csrf".to_string(),
            field_name: "_csrf_token".to_string(),
            header_name: "x-csrf-token".to_string(),
            cookie_max_age: 86400,
            token_length: 32,
            secure: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_values() {
        let config = CsrfConfig::default();
        assert_eq!(config.cookie_name, "_csrf");
        assert_eq!(config.field_name, "_csrf_token");
        assert_eq!(config.header_name, "x-csrf-token");
        assert_eq!(config.cookie_max_age, 86400);
        assert_eq!(config.token_length, 32);
        assert!(!config.secure);
    }

    #[test]
    fn partial_yaml_deserialization() {
        let yaml = r#"
cookie_name: "my_csrf"
secure: true
"#;
        let config: CsrfConfig = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(config.cookie_name, "my_csrf");
        assert!(config.secure);
        // Defaults preserved
        assert_eq!(config.field_name, "_csrf_token");
        assert_eq!(config.header_name, "x-csrf-token");
        assert_eq!(config.cookie_max_age, 86400);
        assert_eq!(config.token_length, 32);
    }
}
