use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Sender identity for outgoing emails.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SenderProfile {
    pub from_name: String,
    pub from_email: String,
    pub reply_to: Option<String>,
}

impl SenderProfile {
    /// Format as `"Name <email>"` for the From header.
    ///
    /// Strips control characters and angle brackets from the name, and control
    /// characters from the email, to prevent header injection.
    pub fn format_address(&self) -> String {
        let safe_name: String = self
            .from_name
            .chars()
            .filter(|c| !c.is_control() && *c != '<' && *c != '>')
            .collect();
        let safe_email: String = self
            .from_email
            .chars()
            .filter(|c| !c.is_control())
            .collect();
        format!("{} <{}>", safe_name.trim(), safe_email.trim())
    }
}

/// A fully-rendered email ready for transport.
#[derive(Debug, Clone)]
pub struct MailMessage {
    pub from: String,
    pub reply_to: Option<String>,
    pub to: Vec<String>,
    pub subject: String,
    pub html: String,
    pub text: String,
}

/// Builder for requesting a templated email send.
#[derive(Debug, Clone)]
pub struct SendEmail {
    pub(crate) template: String,
    pub(crate) to: Vec<String>,
    pub(crate) locale: Option<String>,
    pub(crate) sender: Option<SenderProfile>,
    pub(crate) context: HashMap<String, serde_json::Value>,
}

impl SendEmail {
    pub fn new(template: &str, to: &str) -> Self {
        Self {
            template: template.to_string(),
            to: vec![to.to_string()],
            locale: None,
            sender: None,
            context: HashMap::new(),
        }
    }

    /// Add an additional recipient.
    pub fn to(mut self, to: &str) -> Self {
        self.to.push(to.to_string());
        self
    }

    pub fn locale(mut self, locale: &str) -> Self {
        self.locale = Some(locale.to_string());
        self
    }

    pub fn sender(mut self, sender: &SenderProfile) -> Self {
        self.sender = Some(sender.clone());
        self
    }

    pub fn var(mut self, key: &str, value: impl Into<serde_json::Value>) -> Self {
        self.context.insert(key.to_string(), value.into());
        self
    }

    pub fn context(mut self, ctx: &HashMap<String, serde_json::Value>) -> Self {
        self.context.extend(ctx.clone());
        self
    }
}

/// Serializable version of `SendEmail` for job payloads.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendEmailPayload {
    pub template: String,
    pub to: Vec<String>,
    pub locale: Option<String>,
    pub sender: Option<SenderProfile>,
    pub context: HashMap<String, serde_json::Value>,
}

impl From<SendEmail> for SendEmailPayload {
    fn from(e: SendEmail) -> Self {
        Self {
            template: e.template,
            to: e.to,
            locale: e.locale,
            sender: e.sender,
            context: e.context,
        }
    }
}

impl From<SendEmailPayload> for SendEmail {
    fn from(p: SendEmailPayload) -> Self {
        Self {
            template: p.template,
            to: p.to,
            locale: p.locale,
            sender: p.sender,
            context: p.context,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sender_profile_serialization_roundtrip() {
        let profile = SenderProfile {
            from_name: "Acme".to_string(),
            from_email: "hi@acme.com".to_string(),
            reply_to: Some("support@acme.com".to_string()),
        };
        let json = serde_json::to_string(&profile).unwrap();
        let back: SenderProfile = serde_json::from_str(&json).unwrap();
        assert_eq!(back.from_name, "Acme");
        assert_eq!(back.reply_to, Some("support@acme.com".to_string()));
    }

    #[test]
    fn sender_profile_format_address() {
        let profile = SenderProfile {
            from_name: "Acme Corp".to_string(),
            from_email: "hi@acme.com".to_string(),
            reply_to: None,
        };
        assert_eq!(profile.format_address(), "Acme Corp <hi@acme.com>");
    }

    #[test]
    fn sender_profile_sanitizes_name() {
        let profile = SenderProfile {
            from_name: "Evil<script>".to_string(),
            from_email: "hi@acme.com".to_string(),
            reply_to: None,
        };
        assert_eq!(profile.format_address(), "Evilscript <hi@acme.com>");
    }

    #[test]
    fn sender_profile_sanitizes_email() {
        let profile = SenderProfile {
            from_name: "Acme".to_string(),
            from_email: "hi@acme.com\r\nBcc: evil@x.com".to_string(),
            reply_to: None,
        };
        let addr = profile.format_address();
        assert!(!addr.contains('\r'));
        assert!(!addr.contains('\n'));
    }

    #[test]
    fn send_email_builder() {
        let email = SendEmail::new("welcome", "user@test.com")
            .locale("de")
            .var("name", "Hans")
            .var("code", "1234");
        assert_eq!(email.template, "welcome");
        assert_eq!(email.to, vec!["user@test.com"]);
        assert_eq!(email.locale.as_deref(), Some("de"));
        assert_eq!(email.context.len(), 2);
    }

    #[test]
    fn send_email_multiple_recipients() {
        let email = SendEmail::new("welcome", "a@test.com")
            .to("b@test.com")
            .to("c@test.com");
        assert_eq!(email.to, vec!["a@test.com", "b@test.com", "c@test.com"]);
    }

    #[test]
    fn send_email_context_merge() {
        let mut brand = HashMap::new();
        brand.insert("logo".to_string(), serde_json::json!("https://logo.png"));
        brand.insert("color".to_string(), serde_json::json!("#ff0000"));

        let email = SendEmail::new("welcome", "u@t.com")
            .context(&brand)
            .var("name", "Alice");
        assert_eq!(email.context.len(), 3);
    }

    #[test]
    fn payload_roundtrip() {
        let email = SendEmail::new("welcome", "u@t.com")
            .to("v@t.com")
            .locale("en")
            .var("name", "Alice");
        let payload = SendEmailPayload::from(email);
        let json = serde_json::to_string(&payload).unwrap();
        let back: SendEmailPayload = serde_json::from_str(&json).unwrap();
        assert_eq!(back.template, "welcome");
        assert_eq!(back.locale.as_deref(), Some("en"));
        assert_eq!(back.to, vec!["u@t.com", "v@t.com"]);

        let email_back = SendEmail::from(back);
        assert_eq!(email_back.to, vec!["u@t.com", "v@t.com"]);
    }
}
