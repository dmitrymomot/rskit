use std::collections::HashMap;

/// Replace `{{key}}` placeholders in `input` with values from `context`.
///
/// - Whitespace inside braces is trimmed: `{{ name }}` matches key `"name"`.
/// - String values are inserted directly; other JSON types use their `to_string()` representation.
/// - Unresolved placeholders are left as-is.
pub fn substitute(input: &str, context: &HashMap<String, serde_json::Value>) -> String {
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
                    match val {
                        serde_json::Value::String(s) => result.push_str(s),
                        other => result.push_str(&other.to_string()),
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
}
