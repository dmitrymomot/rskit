use std::any::{Any, TypeId};

/// Trait implemented by `#[derive(modo::Sanitize)]` to sanitize struct fields in place.
pub trait Sanitize {
    fn sanitize(&mut self);
}

/// Registration entry for an auto-sanitizer, collected via `inventory`.
pub struct SanitizerRegistration {
    pub type_id: TypeId,
    pub sanitize: fn(&mut dyn Any),
}

inventory::collect!(SanitizerRegistration);

/// Auto-sanitize a value if a sanitizer is registered for its type.
/// Called by extractors (Form, Json, MultipartForm) during request parsing.
/// No-op if no `#[derive(Sanitize)]` was used on the type.
pub fn auto_sanitize<T: Any + 'static>(value: &mut T) {
    for reg in inventory::iter::<SanitizerRegistration> {
        if reg.type_id == TypeId::of::<T>() {
            (reg.sanitize)(value);
            return;
        }
    }
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
///
/// # Warning
///
/// This is cosmetic stripping only — **not** a security sanitizer.
/// Do **not** rely on this for XSS prevention. HTML entities (e.g. `&amp;`)
/// pass through unchanged, and unclosed tags will swallow all content after
/// the opening `<`. Use a dedicated HTML sanitizer library for security.
pub fn strip_html_tags(s: String) -> String {
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

/// Normalize an email address by lowercasing and stripping the `+tag` from the local part.
/// `"User+Tag@Example.COM"` becomes `"user@example.com"`.
/// Returns the lowercased string unchanged if there is no `@` or no `+` in the local part.
pub fn normalize_email(s: String) -> String {
    let s = s.to_lowercase();
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
