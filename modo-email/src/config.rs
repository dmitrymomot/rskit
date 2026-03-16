use serde::Deserialize;

/// Which delivery backend to use for outgoing email.
///
/// Serialized as lowercase strings (`"smtp"`, `"resend"`) in YAML/JSON config.
#[derive(Debug, Clone, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TransportBackend {
    /// Send via SMTP (default). Requires the `smtp` feature.
    #[default]
    Smtp,
    /// Send via the Resend HTTP API. Requires the `resend` feature.
    Resend,
}

/// Top-level email configuration loaded from YAML or environment.
///
/// All fields implement `Default`, so partial YAML is valid — only override
/// what differs from the defaults.
///
/// Feature-gated fields (`smtp`, `resend`) are only present when the
/// corresponding Cargo feature is enabled.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct EmailConfig {
    /// Which transport backend to use. Defaults to `smtp`.
    pub transport: TransportBackend,
    /// Directory that contains `.md` template files. Defaults to `"emails"`.
    pub templates_path: String,
    /// Display name used in the `From` header when no per-email sender is set.
    pub default_from_name: String,
    /// Email address used in the `From` header when no per-email sender is set.
    pub default_from_email: String,
    /// Optional default `Reply-To` address.
    pub default_reply_to: Option<String>,
    /// Whether to cache compiled email templates. Defaults to `true`.
    /// Set to `false` in development for live template reloading.
    pub cache_templates: bool,
    /// Maximum number of compiled templates to keep in cache.
    /// Defaults to `100`. Only used when `cache_templates` is `true`.
    pub template_cache_size: usize,

    /// SMTP connection settings. Requires the `smtp` feature.
    #[cfg(feature = "smtp")]
    pub smtp: SmtpConfig,

    /// Resend API settings. Requires the `resend` feature.
    #[cfg(feature = "resend")]
    pub resend: ResendConfig,
}

impl Default for EmailConfig {
    fn default() -> Self {
        Self {
            transport: TransportBackend::default(),
            templates_path: "emails".to_string(),
            default_from_name: String::new(),
            default_from_email: String::new(),
            default_reply_to: None,
            cache_templates: true,
            template_cache_size: 100,
            #[cfg(feature = "smtp")]
            smtp: SmtpConfig::default(),
            #[cfg(feature = "resend")]
            resend: ResendConfig::default(),
        }
    }
}

/// TLS mode for SMTP connections.
#[cfg(feature = "smtp")]
#[derive(Debug, Clone, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SmtpSecurity {
    /// No TLS — plaintext connection (use only for local dev or trusted networks).
    None,
    /// Upgrade a plaintext connection to TLS via the STARTTLS command (port 587).
    #[default]
    StartTls,
    /// Connect with TLS from the start — SMTPS (port 465).
    ImplicitTls,
}

/// SMTP connection settings. Requires the `smtp` feature.
#[cfg(feature = "smtp")]
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct SmtpConfig {
    /// SMTP server hostname. Defaults to `"localhost"`.
    pub host: String,
    /// SMTP server port. Defaults to `587`.
    pub port: u16,
    /// SMTP authentication username.
    pub username: String,
    /// SMTP authentication password.
    pub password: String,
    /// TLS security mode. Defaults to `StartTls`.
    pub security: SmtpSecurity,
}

#[cfg(feature = "smtp")]
impl Default for SmtpConfig {
    fn default() -> Self {
        Self {
            host: "localhost".to_string(),
            port: 587,
            username: String::new(),
            password: String::new(),
            security: SmtpSecurity::default(),
        }
    }
}

/// Resend HTTP API settings. Requires the `resend` feature.
#[cfg(feature = "resend")]
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct ResendConfig {
    /// Resend API key (starts with `re_`).
    pub api_key: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_values() {
        let config = EmailConfig::default();
        assert_eq!(config.templates_path, "emails");
        assert_eq!(config.default_from_name, "");
        assert_eq!(config.default_from_email, "");
        assert!(config.default_reply_to.is_none());
        assert_eq!(config.transport, TransportBackend::Smtp);
    }

    #[test]
    fn partial_yaml_deserialization() {
        let yaml = r#"
templates_path: "mail"
default_from_name: "Acme"
default_from_email: "hi@acme.com"
"#;
        let config: EmailConfig = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(config.templates_path, "mail");
        assert_eq!(config.default_from_name, "Acme");
        assert_eq!(config.default_from_email, "hi@acme.com");
        assert_eq!(config.transport, TransportBackend::Smtp);
    }

    #[test]
    fn transport_backend_deserialization() {
        let yaml = "transport: resend";
        let config: EmailConfig = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(config.transport, TransportBackend::Resend);
    }

    #[cfg(feature = "smtp")]
    #[test]
    fn smtp_security_none_deserialization() {
        let yaml = r#"
transport: smtp
smtp:
  host: "localhost"
  security: none
"#;
        let config: EmailConfig = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(config.smtp.security, SmtpSecurity::None);
    }

    #[cfg(feature = "smtp")]
    #[test]
    fn smtp_security_starttls_deserialization() {
        let yaml = r#"
transport: smtp
smtp:
  host: "mail.example.com"
  security: starttls
"#;
        let config: EmailConfig = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(config.smtp.security, SmtpSecurity::StartTls);
    }

    #[cfg(feature = "smtp")]
    #[test]
    fn smtp_security_implicit_tls_deserialization() {
        let yaml = r#"
transport: smtp
smtp:
  host: "mail.example.com"
  port: 465
  security: implicit_tls
"#;
        let config: EmailConfig = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(config.smtp.security, SmtpSecurity::ImplicitTls);
        assert_eq!(config.smtp.port, 465);
    }

    #[cfg(feature = "smtp")]
    #[test]
    fn smtp_security_default_is_starttls() {
        let config = SmtpConfig::default();
        assert_eq!(config.security, SmtpSecurity::StartTls);
    }

    #[test]
    fn cache_config_deserialization() {
        let yaml = r#"
cache_templates: false
template_cache_size: 50
"#;
        let config: EmailConfig = serde_yaml_ng::from_str(yaml).unwrap();
        assert!(!config.cache_templates);
        assert_eq!(config.template_cache_size, 50);
    }

    #[test]
    fn cache_config_defaults() {
        let config = EmailConfig::default();
        assert!(config.cache_templates);
        assert_eq!(config.template_cache_size, 100);
    }
}
