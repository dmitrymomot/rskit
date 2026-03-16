/// Replace `{key}` placeholders in `template` with values from `vars`.
///
/// Uses single-pass substitution: the output of one replacement is never
/// re-scanned, preventing recursive variable expansion. Unresolved
/// placeholders are left as-is.
pub(crate) fn interpolate<K, V>(template: &str, vars: &[(K, V)]) -> String
where
    K: AsRef<str>,
    V: AsRef<str>,
{
    let mut result = String::with_capacity(template.len());
    let mut chars = template.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '{' {
            let mut key = String::new();
            let mut found_close = false;

            for inner in chars.by_ref() {
                if inner == '}' {
                    found_close = true;
                    break;
                }
                key.push(inner);
            }

            if found_close {
                if let Some((_, val)) = vars.iter().find(|(k, _)| k.as_ref() == key) {
                    result.push_str(val.as_ref());
                } else {
                    result.push('{');
                    result.push_str(&key);
                    result.push('}');
                }
            } else {
                result.push('{');
                result.push_str(&key);
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

    #[test]
    fn no_recursive_expansion() {
        // "admin" value contains {role}, but {role} should NOT be expanded
        let vars = vec![
            ("name", "Alice"),
            ("role", "admin"),
            ("admin", "value contains {role}"),
        ];
        let result = interpolate("Hello {name}, you are {admin}", &vars);
        assert_eq!(result, "Hello Alice, you are value contains {role}");
    }

    #[test]
    fn basic_substitution() {
        let vars = vec![("name", "Bob"), ("count", "5")];
        let result = interpolate("Hello {name}, you have {count} items", &vars);
        assert_eq!(result, "Hello Bob, you have 5 items");
    }

    #[test]
    fn no_vars_passthrough() {
        let result = interpolate("Hello world", &[("unused", "val")]);
        assert_eq!(result, "Hello world");
    }

    #[test]
    fn unresolved_placeholders_left_as_is() {
        let result = interpolate("Hello {name}", &[("other", "val")]);
        assert_eq!(result, "Hello {name}");
    }

    #[test]
    fn empty_vars() {
        let vars: Vec<(&str, &str)> = vec![];
        let result = interpolate("Hello {name}", &vars);
        assert_eq!(result, "Hello {name}");
    }

    #[test]
    fn unclosed_brace_at_eof() {
        let vars: Vec<(&str, &str)> = vec![];
        let result = interpolate("Hello {name", &vars);
        assert_eq!(result, "Hello {name");
    }
}
