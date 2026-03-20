use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct EmailConfig {
    pub templates_path: String,
    pub layouts_path: String,
    pub default_from_name: String,
    pub default_from_email: String,
    pub default_reply_to: Option<String>,
    pub default_locale: String,
    pub cache_templates: bool,
    pub template_cache_size: usize,
    pub smtp: SmtpConfig,
}

impl Default for EmailConfig {
    fn default() -> Self {
        Self {
            templates_path: "emails".into(),
            layouts_path: "emails/layouts".into(),
            default_from_name: String::new(),
            default_from_email: String::new(),
            default_reply_to: None,
            default_locale: "en".into(),
            cache_templates: true,
            template_cache_size: 100,
            smtp: SmtpConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct SmtpConfig {
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<String>,
    pub security: SmtpSecurity,
}

impl Default for SmtpConfig {
    fn default() -> Self {
        Self {
            host: "localhost".into(),
            port: 587,
            username: None,
            password: None,
            security: SmtpSecurity::default(),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum SmtpSecurity {
    #[default]
    StartTls,
    Tls,
    None,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn email_config_defaults() {
        let config = EmailConfig::default();
        assert_eq!(config.templates_path, "emails");
        assert_eq!(config.layouts_path, "emails/layouts");
        assert_eq!(config.default_from_name, "");
        assert_eq!(config.default_from_email, "");
        assert!(config.default_reply_to.is_none());
        assert_eq!(config.default_locale, "en");
        assert!(config.cache_templates);
        assert_eq!(config.template_cache_size, 100);
    }

    #[test]
    fn smtp_config_defaults() {
        let config = SmtpConfig::default();
        assert_eq!(config.host, "localhost");
        assert_eq!(config.port, 587);
        assert!(config.username.is_none());
        assert!(config.password.is_none());
        assert_eq!(config.security, SmtpSecurity::StartTls);
    }

    #[test]
    fn email_config_from_yaml() {
        let yaml = r#"
            templates_path: custom/emails
            default_from_name: TestApp
            default_from_email: test@example.com
            default_reply_to: reply@example.com
            default_locale: uk
            cache_templates: false
            template_cache_size: 50
            smtp:
              host: smtp.example.com
              port: 465
              username: user
              password: pass
              security: tls
        "#;
        let config: EmailConfig = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(config.templates_path, "custom/emails");
        assert_eq!(config.default_from_name, "TestApp");
        assert_eq!(config.default_from_email, "test@example.com");
        assert_eq!(
            config.default_reply_to.as_deref(),
            Some("reply@example.com")
        );
        assert_eq!(config.default_locale, "uk");
        assert!(!config.cache_templates);
        assert_eq!(config.template_cache_size, 50);
        assert_eq!(config.smtp.host, "smtp.example.com");
        assert_eq!(config.smtp.port, 465);
        assert_eq!(config.smtp.username.as_deref(), Some("user"));
        assert_eq!(config.smtp.password.as_deref(), Some("pass"));
        assert_eq!(config.smtp.security, SmtpSecurity::Tls);
    }

    #[test]
    fn email_config_partial_yaml_uses_defaults() {
        let yaml = r#"
            default_from_email: noreply@app.com
        "#;
        let config: EmailConfig = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(config.templates_path, "emails");
        assert_eq!(config.default_from_email, "noreply@app.com");
        assert_eq!(config.smtp.host, "localhost");
        assert_eq!(config.smtp.port, 587);
    }

    #[test]
    fn smtp_security_none_variant() {
        let yaml = r#"security: none"#;
        let config: SmtpConfig = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(config.security, SmtpSecurity::None);
    }
}
