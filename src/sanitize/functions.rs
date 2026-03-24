/// Trims leading and trailing whitespace in place.
pub fn trim(s: &mut String) {
    *s = s.trim().to_string();
}

/// Trims leading and trailing whitespace and converts to lowercase in place.
pub fn trim_lowercase(s: &mut String) {
    *s = s.trim().to_lowercase();
}

/// Collapses consecutive whitespace characters (spaces, tabs, newlines) into a
/// single space.
///
/// Leading whitespace at the start of the string is preserved as a single space.
/// Use [`trim`] after this function if leading/trailing whitespace must also be
/// removed.
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

/// Strips all HTML tags and decodes common entities, leaving only plain text.
///
/// Script and style element content (including their tags) is discarded
/// entirely.  Whitespace is collapsed and trimmed in the resulting plain text.
pub fn strip_html(s: &mut String) {
    *s = super::html::html_to_text(s);
}

/// Truncates the string to at most `max_chars` Unicode scalar values in place.
///
/// If the string is shorter than `max_chars` it is left unchanged.
pub fn truncate(s: &mut String, max_chars: usize) {
    if let Some((idx, _)) = s.char_indices().nth(max_chars) {
        s.truncate(idx);
    }
}

/// Normalizes an email address in place: trims whitespace, lowercases, and
/// strips the `+tag` portion from the local part.
///
/// For example, `"  User+Tag@Example.COM  "` becomes `"user@example.com"`.
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
