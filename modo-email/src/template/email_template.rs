use serde::Deserialize;

/// A parsed email template with subject, body, and optional layout.
pub struct EmailTemplate {
    /// Subject line with `{{var}}` placeholders (not yet substituted).
    pub subject: String,
    /// Markdown body with `{{var}}` placeholders (not yet substituted).
    pub body: String,
    /// Name of the layout to wrap the body in, or `None` to use `"default"`.
    pub layout: Option<String>,
}

#[derive(Deserialize)]
struct Frontmatter {
    subject: String,
    layout: Option<String>,
}

impl EmailTemplate {
    /// Parse a raw template string with YAML frontmatter delimited by `---`.
    ///
    /// The frontmatter must contain at least a `subject` field.
    /// An optional `layout` field specifies which layout to wrap the body in.
    pub fn parse(raw: &str) -> Result<Self, modo::Error> {
        let raw = raw.trim();
        if !raw.starts_with("---") {
            return Err(modo::Error::internal(
                "email template must start with YAML frontmatter (---)",
            ));
        }

        let after_first = &raw[3..];
        let end = after_first.find("---").ok_or_else(|| {
            modo::Error::internal("email template frontmatter missing closing ---")
        })?;

        let yaml = &after_first[..end];
        let body = &after_first[end + 3..];

        let fm: Frontmatter = serde_yaml_ng::from_str(yaml)
            .map_err(|e| modo::Error::internal(format!("invalid frontmatter: {e}")))?;

        Ok(Self {
            subject: fm.subject,
            body: body.to_string(),
            layout: fm.layout,
        })
    }
}

/// Trait for loading email templates by name and locale.
///
/// Implement this to load templates from a database, cache, or any source
/// other than the filesystem. Pass the implementation to [`mailer_with`](crate::mailer_with).
pub trait TemplateProvider: Send + Sync + 'static {
    /// Return the template identified by `name`, resolving to the given `locale`
    /// when available. Pass an empty string for `locale` to request the default.
    fn get(&self, name: &str, locale: &str) -> Result<EmailTemplate, modo::Error>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_template_with_frontmatter() {
        let raw = "---\nsubject: \"Hello {{name}}\"\nlayout: custom\n---\n\nBody here.";
        let tpl = EmailTemplate::parse(raw).unwrap();
        assert_eq!(tpl.subject, "Hello {{name}}");
        assert_eq!(tpl.layout.as_deref(), Some("custom"));
        assert_eq!(tpl.body.trim(), "Body here.");
    }

    #[test]
    fn parse_template_default_layout() {
        let raw = "---\nsubject: \"Hi\"\n---\n\nContent.";
        let tpl = EmailTemplate::parse(raw).unwrap();
        assert_eq!(tpl.subject, "Hi");
        assert!(tpl.layout.is_none());
    }

    #[test]
    fn parse_template_missing_subject() {
        let raw = "---\nlayout: default\n---\n\nNo subject.";
        let result = EmailTemplate::parse(raw);
        assert!(result.is_err());
    }

    #[test]
    fn parse_template_no_frontmatter() {
        let raw = "Just markdown, no frontmatter.";
        let result = EmailTemplate::parse(raw);
        assert!(result.is_err());
    }

    #[test]
    fn parse_empty_input() {
        let result = EmailTemplate::parse("");
        assert!(result.is_err());
    }

    #[test]
    fn parse_only_opening_delimiter() {
        let result = EmailTemplate::parse("---");
        assert!(result.is_err());
    }

    #[test]
    fn parse_body_contains_triple_dash() {
        let raw = "---\nsubject: \"Hi\"\n---\nBody\n---\nMore body";
        let tpl = EmailTemplate::parse(raw).unwrap();
        assert_eq!(tpl.subject, "Hi");
        assert!(tpl.body.contains("Body"));
        assert!(tpl.body.contains("---"));
        assert!(tpl.body.contains("More body"));
    }

    #[test]
    fn parse_empty_body() {
        let raw = "---\nsubject: \"Hi\"\n---";
        let tpl = EmailTemplate::parse(raw).unwrap();
        assert_eq!(tpl.subject, "Hi");
        assert!(tpl.body.trim().is_empty());
    }

    #[test]
    fn parse_unicode_subject_and_body() {
        let raw = "---\nsubject: \"Willkommen 🎉\"\n---\n你好世界";
        let tpl = EmailTemplate::parse(raw).unwrap();
        assert_eq!(tpl.subject, "Willkommen 🎉");
        assert!(tpl.body.contains("你好世界"));
    }

    #[test]
    fn parse_extra_whitespace_around_frontmatter() {
        let raw = "  \n---\nsubject: \"Hi\"\n---\nBody  \n  ";
        let tpl = EmailTemplate::parse(raw).unwrap();
        assert_eq!(tpl.subject, "Hi");
        assert!(tpl.body.contains("Body"));
    }

    #[test]
    fn parse_extra_fields_ignored() {
        let raw = "---\nsubject: \"Hi\"\npriority: high\ncustom_key: value\n---\nBody";
        let tpl = EmailTemplate::parse(raw).unwrap();
        assert_eq!(tpl.subject, "Hi");
        assert!(tpl.body.contains("Body"));
    }
}
