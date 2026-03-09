pub mod filesystem;
pub mod layout;
pub mod markdown;

use serde::Deserialize;

/// A parsed email template with subject, body, and optional layout.
pub struct EmailTemplate {
    pub subject: String,
    pub body: String,
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
                "Email template must start with YAML frontmatter (---)",
            ));
        }

        let after_first = &raw[3..];
        let end = after_first.find("---").ok_or_else(|| {
            modo::Error::internal("Email template frontmatter missing closing ---")
        })?;

        let yaml = &after_first[..end];
        let body = &after_first[end + 3..];

        let fm: Frontmatter = serde_yaml_ng::from_str(yaml)
            .map_err(|e| modo::Error::internal(format!("Invalid frontmatter: {e}")))?;

        Ok(Self {
            subject: fm.subject,
            body: body.to_string(),
            layout: fm.layout,
        })
    }
}

/// Trait for loading email templates by name and locale.
pub trait TemplateProvider: Send + Sync + 'static {
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
}
