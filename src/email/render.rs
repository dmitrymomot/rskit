use crate::{Error, Result};
use regex::Regex;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::LazyLock;

static CSS_INLINER: LazyLock<css_inline::CSSInliner<'static>> = LazyLock::new(|| {
    css_inline::CSSInliner::options()
        .keep_style_tags(true)
        .load_remote_stylesheets(false)
        .build()
});

/// Parsed YAML frontmatter from an email template.
#[derive(Debug, Deserialize)]
pub struct Frontmatter {
    /// The email subject line (after variable substitution).
    pub subject: String,
    /// Layout name to apply. Defaults to `"base"`.
    #[serde(default = "default_layout")]
    pub layout: String,
}

fn default_layout() -> String {
    "base".into()
}

static VAR_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\{\{([a-zA-Z_][a-zA-Z0-9_]*)\}\}").expect("static regex"));

/// Replace all `{{var}}` in the input string with raw values from the vars map.
/// Missing variables are replaced with empty strings.
pub fn substitute(input: &str, vars: &HashMap<String, String>) -> String {
    VAR_RE
        .replace_all(input, |caps: &regex::Captures| {
            vars.get(&caps[1]).cloned().unwrap_or_default()
        })
        .into_owned()
}

/// Escape HTML special characters for safe interpolation into HTML attributes and text.
pub(crate) fn escape_html(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Inline CSS declarations from `<style>` blocks into element `style=""`
/// attributes while preserving the original `<style>` block so `@media`
/// rules (dark mode, mobile) still apply on clients that honour them.
///
/// Existing inline `style=""` on an element wins over rules from `<style>`
/// per standard CSS specificity.
///
/// # Errors
///
/// Returns [`Error::internal`] when the HTML cannot be parsed. Generated
/// layouts are well-formed; callers surfacing this error are almost always
/// looking at a malformed custom layout.
pub fn inline_css_pass(html: &str) -> Result<String> {
    CSS_INLINER
        .inline(html)
        .map_err(|e| Error::internal(format!("css inline failed: {e}")).chain(e))
}

/// Split a template string into frontmatter and body.
/// Template must start with `---\n` and have a closing `---\n`.
/// Normalizes CRLF line endings before parsing.
pub fn parse_frontmatter(template: &str) -> Result<(Frontmatter, String)> {
    let normalized = template.replace("\r\n", "\n");
    let trimmed = normalized.trim_start();

    if !trimmed.starts_with("---") {
        return Err(Error::bad_request(
            "email template missing frontmatter delimiter '---'",
        ));
    }

    let after_first = &trimmed[3..];
    let after_first = after_first.strip_prefix('\n').unwrap_or(after_first);

    let end = after_first.find("\n---").ok_or_else(|| {
        Error::bad_request("email template missing closing frontmatter delimiter '---'")
    })?;

    let yaml = &after_first[..end];
    let body = &after_first[end + 4..]; // skip "\n---"
    let body = body.strip_prefix('\n').unwrap_or(body);

    let frontmatter: Frontmatter = serde_yaml_ng::from_str(yaml)
        .map_err(|e| Error::internal(format!("failed to parse email frontmatter: {e}")))?;

    if frontmatter.subject.is_empty() {
        return Err(Error::bad_request(
            "email template missing required field 'subject'",
        ));
    }

    Ok((frontmatter, body.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn substitute_replaces_known_vars() {
        let mut vars = HashMap::new();
        vars.insert("name".into(), "Dmytro".into());
        vars.insert("product".into(), "Modo".into());
        let result = substitute("Hello {{name}}, welcome to {{product}}!", &vars);
        assert_eq!(result, "Hello Dmytro, welcome to Modo!");
    }

    #[test]
    fn substitute_missing_var_becomes_empty() {
        let vars = HashMap::new();
        let result = substitute("Hello {{name}}!", &vars);
        assert_eq!(result, "Hello !");
    }

    #[test]
    fn substitute_preserves_invalid_var_names() {
        let vars = HashMap::new();
        let result = substitute("Hello {{123invalid}}!", &vars);
        assert_eq!(result, "Hello {{123invalid}}!");
    }

    #[test]
    fn substitute_no_vars_in_template() {
        let vars = HashMap::new();
        let result = substitute("Hello world!", &vars);
        assert_eq!(result, "Hello world!");
    }

    #[test]
    fn substitute_special_chars_in_value() {
        let mut vars = HashMap::new();
        vars.insert("name".into(), "<b>Bold</b>".into());
        let result = substitute("Hello {{name}}!", &vars);
        assert_eq!(result, "Hello <b>Bold</b>!");
    }

    #[test]
    fn substitute_vars_in_frontmatter() {
        let mut vars = HashMap::new();
        vars.insert("product".into(), "Modo".into());
        vars.insert("name".into(), "Dmytro".into());
        let template = "---\nsubject: \"Welcome to {{product}}, {{name}}!\"\n---\nBody";
        let result = substitute(template, &vars);
        assert!(result.contains("Welcome to Modo, Dmytro!"));
    }

    #[test]
    fn parse_frontmatter_valid() {
        let template = "---\nsubject: Welcome!\nlayout: custom\n---\nHello body";
        let (fm, body) = parse_frontmatter(template).unwrap();
        assert_eq!(fm.subject, "Welcome!");
        assert_eq!(fm.layout, "custom");
        assert_eq!(body, "Hello body");
    }

    #[test]
    fn parse_frontmatter_default_layout() {
        let template = "---\nsubject: Hello\n---\nBody";
        let (fm, _) = parse_frontmatter(template).unwrap();
        assert_eq!(fm.layout, "base");
    }

    #[test]
    fn parse_frontmatter_empty_body() {
        let template = "---\nsubject: Hello\n---\n";
        let (fm, body) = parse_frontmatter(template).unwrap();
        assert_eq!(fm.subject, "Hello");
        assert!(body.is_empty());
    }

    #[test]
    fn parse_frontmatter_missing_delimiter() {
        let result = parse_frontmatter("No frontmatter here");
        assert!(result.is_err());
    }

    #[test]
    fn parse_frontmatter_missing_closing_delimiter() {
        let result = parse_frontmatter("---\nsubject: Hello\nNo closing");
        assert!(result.is_err());
    }

    #[test]
    fn parse_frontmatter_missing_subject() {
        let result = parse_frontmatter("---\nlayout: base\n---\nBody");
        assert!(result.is_err());
    }

    #[test]
    fn parse_frontmatter_empty_subject() {
        let result = parse_frontmatter("---\nsubject: \"\"\n---\nBody");
        assert!(result.is_err());
    }

    #[test]
    fn escape_html_basic() {
        assert_eq!(
            escape_html(r#"<b>"Bold" & <i>italic</i></b>"#),
            "&lt;b&gt;&quot;Bold&quot; &amp; &lt;i&gt;italic&lt;/i&gt;&lt;/b&gt;"
        );
    }

    #[test]
    fn escape_html_empty() {
        assert_eq!(escape_html(""), "");
    }

    #[test]
    fn escape_html_no_special_chars() {
        assert_eq!(escape_html("Hello world"), "Hello world");
    }

    #[test]
    fn parse_frontmatter_crlf() {
        let template = "---\r\nsubject: Welcome!\r\n---\r\nHello body";
        let (fm, body) = parse_frontmatter(template).unwrap();
        assert_eq!(fm.subject, "Welcome!");
        assert_eq!(body, "Hello body");
    }

    #[test]
    fn parse_frontmatter_leading_whitespace() {
        let template = "  \n---\nsubject: Hello\n---\nBody";
        let (fm, body) = parse_frontmatter(template).unwrap();
        assert_eq!(fm.subject, "Hello");
        assert_eq!(body, "Body");
    }

    #[test]
    fn parse_frontmatter_malformed_yaml() {
        let result = parse_frontmatter("---\nsubject: [broken\n---\nBody");
        assert!(result.is_err());
    }

    #[test]
    fn parse_frontmatter_body_with_triple_dash() {
        let template = "---\nsubject: Hello\n---\nBefore\n---\nAfter";
        let (fm, body) = parse_frontmatter(template).unwrap();
        assert_eq!(fm.subject, "Hello");
        assert_eq!(body, "Before\n---\nAfter");
    }

    #[test]
    fn inline_css_inlines_style_rules() {
        let html =
            r#"<html><head><style>h1 { color: red; }</style></head><body><h1>X</h1></body></html>"#;
        let inlined = inline_css_pass(html).unwrap();
        assert!(
            inlined.contains("style=\"color: red") || inlined.contains("style=\"color:red"),
            "expected inlined h1 style, got: {inlined}"
        );
    }

    #[test]
    fn inline_css_preserves_media_queries() {
        let html = r#"<html><head><style>@media (prefers-color-scheme: dark) { body { color: white; } }</style></head><body>x</body></html>"#;
        let inlined = inline_css_pass(html).unwrap();
        assert!(inlined.contains("prefers-color-scheme: dark"));
    }

    #[test]
    fn inline_css_inline_attr_wins_over_style() {
        let html = r#"<html><head><style>p { color: red; }</style></head><body><p style="color: blue;">x</p></body></html>"#;
        let inlined = inline_css_pass(html).unwrap();
        // Inline `blue` must still be present; `red` must NOT appear as an applied color.
        assert!(inlined.contains("color: blue") || inlined.contains("color:blue"));
        // The inliner should not emit `red` in the element's style attribute.
        // Note: `red` remains inside the <style> block (that's fine), so we check
        // only the portion after </style>.
        let style_end = inlined.find("</style>").expect("style retained");
        let after_style = &inlined[style_end..];
        assert!(
            !after_style.contains("color: red"),
            "red leaked into element style, got: {after_style:.500}"
        );
        assert!(
            !after_style.contains("color:red"),
            "red leaked into element style (no space), got: {after_style:.500}"
        );
    }
}
