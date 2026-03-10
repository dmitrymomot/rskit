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
    pub max_body_bytes: usize,
}

impl Default for CsrfConfig {
    fn default() -> Self {
        Self {
            cookie_name: "_csrf".to_string(),
            field_name: "_csrf_token".to_string(),
            header_name: "x-csrf-token".to_string(),
            cookie_max_age: 86400,
            token_length: 32,
            secure: true,
            max_body_bytes: 1_048_576,
        }
    }
}

impl CsrfConfig {
    /// Validate that cookie_name, field_name, and header_name contain only
    /// safe characters (alphanumeric, `-`, `_`) and are non-empty.
    pub fn validate(&self) -> Result<(), String> {
        Self::validate_name("cookie_name", &self.cookie_name)?;
        Self::validate_name("field_name", &self.field_name)?;
        Self::validate_name("header_name", &self.header_name)?;
        Ok(())
    }

    fn validate_name(label: &str, value: &str) -> Result<(), String> {
        if value.is_empty() {
            return Err(format!("CSRF {label} must not be empty"));
        }
        if !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            return Err(format!(
                "CSRF {label} contains invalid characters (only alphanumeric, '-', '_' allowed): {value:?}"
            ));
        }
        Ok(())
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
        assert!(config.secure);
        assert_eq!(config.max_body_bytes, 1_048_576);
    }

    #[test]
    fn validate_default_passes() {
        CsrfConfig::default().validate().unwrap();
    }

    #[test]
    fn validate_rejects_semicolon() {
        let config = CsrfConfig {
            cookie_name: "_csrf;injection".to_string(),
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn validate_rejects_equals() {
        let config = CsrfConfig {
            cookie_name: "name=value".to_string(),
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn validate_rejects_newline() {
        let config = CsrfConfig {
            header_name: "x-csrf\ninjection".to_string(),
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn validate_rejects_space() {
        let config = CsrfConfig {
            field_name: "csrf token".to_string(),
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn validate_rejects_empty() {
        let config = CsrfConfig {
            cookie_name: String::new(),
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn validate_accepts_hyphen_underscore() {
        let config = CsrfConfig {
            cookie_name: "my-csrf_token".to_string(),
            field_name: "csrf-field_name".to_string(),
            header_name: "x-csrf-token".to_string(),
            ..Default::default()
        };
        config.validate().unwrap();
    }

    #[test]
    fn partial_yaml_deserialization() {
        let yaml = r#"
cookie_name: "my_csrf"
secure: false
max_body_bytes: 2097152
"#;
        let config: CsrfConfig = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(config.cookie_name, "my_csrf");
        assert!(!config.secure);
        assert_eq!(config.max_body_bytes, 2_097_152);
        // Defaults preserved
        assert_eq!(config.field_name, "_csrf_token");
        assert_eq!(config.header_name, "x-csrf-token");
        assert_eq!(config.cookie_max_age, 86400);
        assert_eq!(config.token_length, 32);
    }
}
