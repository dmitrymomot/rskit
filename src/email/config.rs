use serde::Deserialize;

/// Top-level email configuration.
///
/// Deserializes from YAML. All fields have sensible defaults, so only the
/// fields that differ from defaults need to be specified.
#[non_exhaustive]
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct EmailConfig {
    /// Directory containing email templates (locale sub-directories allowed).
    /// Default: `"emails"`.
    pub templates_path: String,
    /// Directory containing custom HTML layout files.
    /// Default: `"emails/layouts"`.
    pub layouts_path: String,
    /// Display name used in the `From` header when no [`SenderProfile`](crate::email::SenderProfile) is set.
    pub default_from_name: String,
    /// Email address used in the `From` header when no [`SenderProfile`](crate::email::SenderProfile) is set.
    pub default_from_email: String,
    /// Optional default `Reply-To` address.
    pub default_reply_to: Option<String>,
    /// BCP 47 locale used when a [`SendEmail`](crate::email::SendEmail) carries no explicit locale.
    /// Default: `"en"`.
    pub default_locale: String,
    /// When `true`, templates are stored in an in-process LRU cache after the
    /// first load. Default: `true`.
    pub cache_templates: bool,
    /// Maximum number of entries in the template LRU cache. Default: `100`.
    pub template_cache_size: usize,
    /// SMTP connection settings.
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

/// SMTP connection settings nested under [`EmailConfig`].
#[non_exhaustive]
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct SmtpConfig {
    /// SMTP server hostname. Default: `"localhost"`.
    pub host: String,
    /// SMTP server port. Default: `587`.
    pub port: u16,
    /// SMTP authentication username. Must be paired with `password`.
    pub username: Option<String>,
    /// SMTP authentication password. Must be paired with `username`.
    pub password: Option<String>,
    /// TLS mode for the SMTP connection. Default: [`SmtpSecurity::StartTls`].
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

/// TLS security mode for the SMTP connection.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum SmtpSecurity {
    /// Upgrade a plain connection to TLS via STARTTLS (default).
    #[default]
    StartTls,
    /// Connect directly over TLS (implicit TLS / port 465).
    Tls,
    /// No encryption — use only in development or with a local relay.
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
