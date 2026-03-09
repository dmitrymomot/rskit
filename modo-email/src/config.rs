use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TransportBackend {
    #[default]
    Smtp,
    Resend,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct EmailConfig {
    pub transport: TransportBackend,
    pub templates_path: String,
    pub default_from_name: String,
    pub default_from_email: String,
    pub default_reply_to: Option<String>,

    #[cfg(feature = "smtp")]
    pub smtp: SmtpConfig,

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
            #[cfg(feature = "smtp")]
            smtp: SmtpConfig::default(),
            #[cfg(feature = "resend")]
            resend: ResendConfig::default(),
        }
    }
}

#[cfg(feature = "smtp")]
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct SmtpConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub tls: bool,
}

#[cfg(feature = "smtp")]
impl Default for SmtpConfig {
    fn default() -> Self {
        Self {
            host: "localhost".to_string(),
            port: 587,
            username: String::new(),
            password: String::new(),
            tls: true,
        }
    }
}

#[cfg(feature = "resend")]
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ResendConfig {
    pub api_key: String,
}

#[cfg(feature = "resend")]
impl Default for ResendConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
        }
    }
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
}
