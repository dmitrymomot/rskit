/// Trait implemented by `#[derive(modo::Sanitize)]` to sanitize struct fields in place.
pub trait Sanitize {
    fn sanitize(&mut self);
}

/// Strip leading and trailing whitespace.
pub fn trim(s: String) -> String {
    s.trim().to_owned()
}

/// Convert to lowercase.
pub fn lowercase(s: String) -> String {
    s.to_lowercase()
}

/// Convert to uppercase.
pub fn uppercase(s: String) -> String {
    s.to_uppercase()
}

/// Remove HTML tags using a simple char-by-char state machine.
pub fn strip_html(s: String) -> String {
    let mut out = String::with_capacity(s.len());
    let mut inside_tag = false;
    for c in s.chars() {
        if c == '<' {
            inside_tag = true;
        } else if c == '>' {
            inside_tag = false;
        } else if !inside_tag {
            out.push(c);
        }
    }
    out
}

/// Collapse multiple consecutive whitespace characters into a single space.
pub fn collapse_whitespace(s: String) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_ws = false;
    for c in s.chars() {
        if c.is_whitespace() {
            if !prev_ws {
                out.push(' ');
                prev_ws = true;
            }
        } else {
            out.push(c);
            prev_ws = false;
        }
    }
    out
}

/// Truncate to at most `max_chars` characters, respecting UTF-8 char boundaries.
pub fn truncate(s: String, max_chars: usize) -> String {
    match s.char_indices().nth(max_chars) {
        Some((idx, _)) => s[..idx].to_owned(),
        None => s,
    }
}

/// Normalize an email address by stripping the `+tag` from the local part.
/// `user+tag@example.com` becomes `user@example.com`.
/// Returns the string unchanged if there is no `@` or no `+` in the local part.
pub fn normalize_email(s: String) -> String {
    let Some(at) = s.find('@') else {
        return s;
    };
    let local = &s[..at];
    let domain = &s[at..];
    match local.find('+') {
        Some(plus) => format!("{}{}", &local[..plus], domain),
        None => s,
    }
}
