//! `{{key}}` variable substitution for email templates.
//!
//! [`substitute`] replaces placeholders without HTML-escaping (for subject lines).
//! [`substitute_html`] HTML-escapes each substituted value (for Markdown bodies).

use std::collections::HashMap;

/// Escape HTML special characters for safe embedding in HTML email output.
fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#x27;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
    out
}

/// Replace `{{key}}` placeholders, HTML-escaping all substituted values.
///
/// Use this for template bodies that will be rendered as HTML. For plain-text
/// contexts (e.g., email subject lines), use [`substitute`] instead.
pub fn substitute_html(input: &str, context: &HashMap<String, serde_json::Value>) -> String {
    substitute_inner(input, context, true)
}

/// Replace `{{key}}` placeholders in `input` with values from `context`.
///
/// - Whitespace inside braces is trimmed: `{{ name }}` matches key `"name"`.
/// - String values are inserted directly; other JSON types use their `to_string()` representation.
/// - Unresolved placeholders are left as-is.
/// - Values are NOT HTML-escaped. For HTML contexts, use [`substitute_html`].
pub fn substitute(input: &str, context: &HashMap<String, serde_json::Value>) -> String {
    substitute_inner(input, context, false)
}

fn substitute_inner(
    input: &str,
    context: &HashMap<String, serde_json::Value>,
    escape_html: bool,
) -> String {
    let mut result = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '{' && chars.peek() == Some(&'{') {
            chars.next(); // consume second {
            let mut key = String::new();
            let mut found_close = false;

            while let Some(ch) = chars.next() {
                if ch == '}' && chars.peek() == Some(&'}') {
                    chars.next();
                    found_close = true;
                    break;
                }
                key.push(ch);
            }

            let key = key.trim();
            if found_close {
                if let Some(val) = context.get(key) {
                    let raw = match val {
                        serde_json::Value::String(s) => s.clone(),
                        other => other.to_string(),
                    };
                    if escape_html {
                        result.push_str(&html_escape(&raw));
                    } else {
                        result.push_str(&raw);
                    }
                } else {
                    result.push_str("{{");
                    result.push_str(key);
                    result.push_str("}}");
                }
            } else {
                result.push_str("{{");
                result.push_str(key);
            }
        } else {
            result.push(ch);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn replace_simple_vars() {
        let mut ctx = HashMap::new();
        ctx.insert("name".to_string(), json!("Alice"));
        ctx.insert("code".to_string(), json!("1234"));

        let result = substitute("Hello {{name}}, code: {{code}}", &ctx);
        assert_eq!(result, "Hello Alice, code: 1234");
    }

    #[test]
    fn unresolved_vars_left_as_is() {
        let ctx = HashMap::new();
        let result = substitute("Hello {{name}}", &ctx);
        assert_eq!(result, "Hello {{name}}");
    }

    #[test]
    fn whitespace_in_braces() {
        let mut ctx = HashMap::new();
        ctx.insert("name".to_string(), json!("Bob"));
        let result = substitute("Hello {{ name }}", &ctx);
        assert_eq!(result, "Hello Bob");
    }

    #[test]
    fn non_string_values() {
        let mut ctx = HashMap::new();
        ctx.insert("count".to_string(), json!(42));
        ctx.insert("active".to_string(), json!(true));
        let result = substitute("Count: {{count}}, active: {{active}}", &ctx);
        assert_eq!(result, "Count: 42, active: true");
    }

    #[test]
    fn empty_input() {
        let ctx = HashMap::new();
        assert_eq!(substitute("", &ctx), "");
    }

    #[test]
    fn no_placeholders_passthrough() {
        let ctx = HashMap::new();
        let result = substitute("Plain text with no braces at all.", &ctx);
        assert_eq!(result, "Plain text with no braces at all.");
    }

    #[test]
    fn unclosed_placeholder_at_eof() {
        let ctx = HashMap::new();
        let result = substitute("Hello {{name", &ctx);
        assert_eq!(result, "Hello {{name");
    }

    #[test]
    fn unclosed_placeholder_mid_text() {
        let ctx = HashMap::new();
        let result = substitute("Hello {{name and more", &ctx);
        assert_eq!(result, "Hello {{name and more");
    }

    #[test]
    fn adjacent_placeholders() {
        let mut ctx = HashMap::new();
        ctx.insert("a".to_string(), json!("X"));
        ctx.insert("b".to_string(), json!("Y"));
        let result = substitute("{{a}}{{b}}", &ctx);
        assert_eq!(result, "XY");
    }

    #[test]
    fn nested_braces() {
        let ctx = HashMap::new();
        let result = substitute("{{{{name}}}}", &ctx);
        // First {{ matches first }} → key "{{name", not found → "{{{{name}}"
        // Remaining }} are literal
        assert_eq!(result, "{{{{name}}}}");
    }

    #[test]
    fn whitespace_only_key() {
        let ctx = HashMap::new();
        let result = substitute("{{ }}", &ctx);
        // Key trims to "", lookup fails → left as {{}}
        assert_eq!(result, "{{}}");
    }

    #[test]
    fn null_json_value() {
        let mut ctx = HashMap::new();
        ctx.insert("val".to_string(), json!(null));
        let result = substitute("Got: {{val}}", &ctx);
        assert_eq!(result, "Got: null");
    }

    #[test]
    fn object_json_value() {
        let mut ctx = HashMap::new();
        ctx.insert("obj".to_string(), json!({"key": "value"}));
        let result = substitute("{{obj}}", &ctx);
        assert_eq!(result, r#"{"key":"value"}"#);
    }

    #[test]
    fn array_json_value() {
        let mut ctx = HashMap::new();
        ctx.insert("arr".to_string(), json!([1, 2, 3]));
        let result = substitute("{{arr}}", &ctx);
        assert_eq!(result, "[1,2,3]");
    }

    #[test]
    fn unicode_in_keys_and_values() {
        let mut ctx = HashMap::new();
        ctx.insert("名前".to_string(), json!("太郎"));
        let result = substitute("Hello {{名前}}", &ctx);
        assert_eq!(result, "Hello 太郎");
    }

    #[test]
    fn html_in_values_is_escaped_for_html() {
        let mut ctx = HashMap::new();
        ctx.insert("name".to_string(), json!("<script>alert('xss')</script>"));
        let result = substitute_html("Hello {{name}}", &ctx);
        assert_eq!(
            result,
            "Hello &lt;script&gt;alert(&#x27;xss&#x27;)&lt;/script&gt;"
        );
    }

    #[test]
    fn ampersand_in_values_is_escaped_for_html() {
        let mut ctx = HashMap::new();
        ctx.insert("company".to_string(), json!("A&B \"Corp\""));
        let result = substitute_html("From {{company}}", &ctx);
        assert_eq!(result, "From A&amp;B &quot;Corp&quot;");
    }

    #[test]
    fn plain_substitute_does_not_escape() {
        let mut ctx = HashMap::new();
        ctx.insert("name".to_string(), json!("O'Brien & Co"));
        let result = substitute("Hello {{name}}", &ctx);
        assert_eq!(result, "Hello O'Brien & Co");
    }
}
