/// Trims leading and trailing whitespace in place.
pub fn trim(s: &mut String) {
    *s = s.trim().to_string();
}

/// Trims whitespace and converts to lowercase in place.
pub fn trim_lowercase(s: &mut String) {
    *s = s.trim().to_lowercase();
}

/// Collapses all consecutive whitespace (spaces, tabs, newlines) into a single space.
pub fn collapse_whitespace(s: &mut String) {
    let mut result = String::with_capacity(s.len());
    let mut prev_was_space = false;
    for ch in s.chars() {
        if ch.is_whitespace() {
            if !prev_was_space {
                result.push(' ');
            }
            prev_was_space = true;
        } else {
            result.push(ch);
            prev_was_space = false;
        }
    }
    *s = result;
}

/// Strips all HTML tags and decodes entities, leaving only plain text.
pub fn strip_html(s: &mut String) {
    *s = super::html::html_to_text(s);
}

/// Truncates the string to at most `max_chars` characters, respecting char boundaries.
pub fn truncate(s: &mut String, max_chars: usize) {
    if let Some((idx, _)) = s.char_indices().nth(max_chars) {
        s.truncate(idx);
    }
}

/// Normalizes an email address: trims whitespace, lowercases, and strips `+tag` suffixes.
pub fn normalize_email(s: &mut String) {
    trim(s);
    *s = s.to_lowercase();
    if let Some((local, domain)) = s.split_once('@') {
        let local = match local.split_once('+') {
            Some((base, _)) => base,
            None => local,
        };
        *s = format!("{local}@{domain}");
    }
}
