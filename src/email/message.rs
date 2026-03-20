use std::collections::HashMap;

/// A rendered email ready for sending.
pub struct RenderedEmail {
    pub subject: String,
    pub html: String,
    pub text: String,
}

/// Overrides the default sender for a specific email.
pub struct SenderProfile {
    pub from_name: String,
    pub from_email: String,
    pub reply_to: Option<String>,
}

/// Builder for composing an email to send.
pub struct SendEmail {
    pub template: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub bcc: Vec<String>,
    pub locale: Option<String>,
    pub vars: HashMap<String, String>,
    pub sender: Option<SenderProfile>,
}

impl SendEmail {
    pub fn new(template: impl Into<String>, to: impl Into<String>) -> Self {
        Self {
            template: template.into(),
            to: vec![to.into()],
            cc: Vec::new(),
            bcc: Vec::new(),
            locale: None,
            vars: HashMap::new(),
            sender: None,
        }
    }

    pub fn to(mut self, addr: impl Into<String>) -> Self {
        self.to.push(addr.into());
        self
    }

    pub fn cc(mut self, addr: impl Into<String>) -> Self {
        self.cc.push(addr.into());
        self
    }

    pub fn bcc(mut self, addr: impl Into<String>) -> Self {
        self.bcc.push(addr.into());
        self
    }

    pub fn locale(mut self, locale: impl Into<String>) -> Self {
        self.locale = Some(locale.into());
        self
    }

    pub fn var(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.vars.insert(key.into(), value.into());
        self
    }

    pub fn sender(mut self, profile: SenderProfile) -> Self {
        self.sender = Some(profile);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_sets_template_and_first_recipient() {
        let email = SendEmail::new("welcome", "user@example.com");
        assert_eq!(email.template, "welcome");
        assert_eq!(email.to, vec!["user@example.com"]);
        assert!(email.cc.is_empty());
        assert!(email.bcc.is_empty());
        assert!(email.locale.is_none());
        assert!(email.vars.is_empty());
        assert!(email.sender.is_none());
    }

    #[test]
    fn builder_chain() {
        let email = SendEmail::new("reset", "a@example.com")
            .to("b@example.com")
            .cc("c@example.com")
            .bcc("d@example.com")
            .locale("uk")
            .var("name", "Dmytro")
            .var("token", "abc123")
            .sender(SenderProfile {
                from_name: "Support".into(),
                from_email: "support@app.com".into(),
                reply_to: Some("help@app.com".into()),
            });
        assert_eq!(email.to, vec!["a@example.com", "b@example.com"]);
        assert_eq!(email.cc, vec!["c@example.com"]);
        assert_eq!(email.bcc, vec!["d@example.com"]);
        assert_eq!(email.locale.as_deref(), Some("uk"));
        assert_eq!(email.vars.get("name").unwrap(), "Dmytro");
        assert_eq!(email.vars.get("token").unwrap(), "abc123");
        let sender = email.sender.unwrap();
        assert_eq!(sender.from_name, "Support");
        assert_eq!(sender.from_email, "support@app.com");
        assert_eq!(sender.reply_to.as_deref(), Some("help@app.com"));
    }

    #[test]
    fn var_overwrites_previous_value() {
        let email = SendEmail::new("t", "a@b.com")
            .var("key", "old")
            .var("key", "new");
        assert_eq!(email.vars.get("key").unwrap(), "new");
    }
}
