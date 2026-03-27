use std::collections::HashMap;

/// A rendered email ready for sending.
///
/// Produced by [`Mailer::render`](crate::email::Mailer::render). Contains the
/// fully substituted subject line, the HTML body (with layout applied), and
/// the plain-text fallback.
pub struct RenderedEmail {
    /// The email subject line, taken from the template frontmatter.
    pub subject: String,
    /// The fully rendered HTML body with layout applied.
    pub html: String,
    /// Plain-text fallback body derived from the Markdown source.
    pub text: String,
}

/// Overrides the default sender for a specific email.
///
/// When attached to a [`SendEmail`] via [`SendEmail::sender`], these values
/// take precedence over the `default_from_*` fields in
/// [`EmailConfig`](crate::email::EmailConfig).
pub struct SenderProfile {
    /// Display name for the `From` header.
    pub from_name: String,
    /// Email address for the `From` header.
    pub from_email: String,
    /// Optional `Reply-To` address.
    pub reply_to: Option<String>,
}

/// Builder for composing an email to send.
///
/// Created with [`SendEmail::new`] and configured via builder-style methods.
/// Pass the completed value to [`Mailer::send`](crate::email::Mailer::send).
pub struct SendEmail {
    /// Template name to render (without locale prefix or `.md` extension).
    pub template: String,
    /// Primary recipients (`To` header).
    pub to: Vec<String>,
    /// Carbon-copy recipients (`Cc` header).
    pub cc: Vec<String>,
    /// Blind carbon-copy recipients (`Bcc` header).
    pub bcc: Vec<String>,
    /// Optional locale override. Falls back to `EmailConfig::default_locale`.
    pub locale: Option<String>,
    /// Variables substituted into `{{var_name}}` placeholders in the template.
    pub vars: HashMap<String, String>,
    /// Optional sender override. Falls back to [`EmailConfig`](crate::email::EmailConfig) defaults.
    pub sender: Option<SenderProfile>,
}

impl SendEmail {
    /// Create a new email builder for the given template and first recipient.
    ///
    /// Additional recipients can be added with [`Self::to`], [`Self::cc`],
    /// and [`Self::bcc`].
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

    /// Add a `To` recipient.
    pub fn to(mut self, addr: impl Into<String>) -> Self {
        self.to.push(addr.into());
        self
    }

    /// Add a `Cc` recipient.
    pub fn cc(mut self, addr: impl Into<String>) -> Self {
        self.cc.push(addr.into());
        self
    }

    /// Add a `Bcc` recipient.
    pub fn bcc(mut self, addr: impl Into<String>) -> Self {
        self.bcc.push(addr.into());
        self
    }

    /// Override the locale used to load this template.
    pub fn locale(mut self, locale: impl Into<String>) -> Self {
        self.locale = Some(locale.into());
        self
    }

    /// Insert or overwrite a template variable.
    ///
    /// The value is substituted for every `{{key}}` occurrence in both
    /// frontmatter and body.
    pub fn var(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.vars.insert(key.into(), value.into());
        self
    }

    /// Override the sender profile for this email.
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
