use crate::error::{Error, Result};

/// Replaces `${VAR}` and `${VAR:default}` placeholders in `input` with
/// values from the process environment.
///
/// - `${VAR}` — substituted with the value of `VAR`; returns an error if `VAR`
///   is not set.
/// - `${VAR:default}` — substituted with the value of `VAR`, or `default` when
///   `VAR` is not set.
///
/// # Errors
///
/// Returns [`crate::Error`] when:
/// - A `${...` placeholder is unclosed (missing `}`).
/// - A required variable (no default) is not set in the process environment.
pub fn substitute_env_vars(input: &str) -> Result<String> {
    let mut result = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '$' && chars.peek() == Some(&'{') {
            chars.next(); // consume '{'
            let mut var_expr = String::new();
            let mut found_close = false;
            for ch in chars.by_ref() {
                if ch == '}' {
                    found_close = true;
                    break;
                }
                var_expr.push(ch);
            }
            if !found_close {
                return Err(Error::internal(format!(
                    "unclosed variable expression '${{{}' in config",
                    var_expr
                )));
            }

            let (var_name, default_val) = match var_expr.split_once(':') {
                Some((name, default)) => (name.trim(), Some(default)),
                None => (var_expr.trim(), None),
            };

            match std::env::var(var_name) {
                Ok(val) => result.push_str(&val),
                Err(_) => match default_val {
                    Some(default) => result.push_str(default),
                    None => {
                        return Err(Error::internal(format!(
                            "required environment variable '{var_name}' is not set"
                        )));
                    }
                },
            }
        } else {
            result.push(ch);
        }
    }

    Ok(result)
}
