use std::fmt::Display;
use std::ops::RangeInclusive;

/// A field-level validator that accumulates errors for a single field.
///
/// String-specific rules are available when `T: AsRef<str>`.
/// Numeric rules (e.g., `range`) are available when `T: PartialOrd + Display`.
pub struct FieldValidator<'a, T> {
    value: &'a T,
    errors: &'a mut Vec<String>,
}

impl<'a, T> FieldValidator<'a, T> {
    pub(crate) fn new(value: &'a T, errors: &'a mut Vec<String>) -> Self {
        Self { value, errors }
    }
}

// --- String rules (T: AsRef<str>) ---

impl<'a, T: AsRef<str>> FieldValidator<'a, T> {
    /// Value must not be empty (after trimming).
    pub fn required(self) -> Self {
        if self.value.as_ref().trim().is_empty() {
            self.errors.push("is required".to_string());
        }
        self
    }

    /// Value must have at least `min` characters.
    pub fn min_length(self, min: usize) -> Self {
        if self.value.as_ref().chars().count() < min {
            self.errors
                .push(format!("must be at least {min} characters"));
        }
        self
    }

    /// Value must have at most `max` characters.
    pub fn max_length(self, max: usize) -> Self {
        if self.value.as_ref().chars().count() > max {
            self.errors
                .push(format!("must be at most {max} characters"));
        }
        self
    }

    /// Value must be a valid email address (simple check).
    pub fn email(self) -> Self {
        let s = self.value.as_ref();
        let is_valid = {
            let parts: Vec<&str> = s.splitn(2, '@').collect();
            parts.len() == 2
                && !parts[0].is_empty()
                && !parts[1].is_empty()
                && parts[1].contains('.')
                && !parts[1].starts_with('.')
                && !parts[1].ends_with('.')
        };
        if !is_valid {
            self.errors
                .push("must be a valid email address".to_string());
        }
        self
    }

    /// Value must be a valid URL (starts with http:// or https:// and contains no spaces).
    pub fn url(self) -> Self {
        let s = self.value.as_ref();
        let is_valid = (s.starts_with("http://") || s.starts_with("https://")) && !s.contains(' ');
        if !is_valid {
            self.errors.push("must be a valid URL".to_string());
        }
        self
    }

    /// Value must be one of the allowed options.
    pub fn one_of(self, options: &[&str]) -> Self {
        let s = self.value.as_ref();
        if !options.contains(&s) {
            let joined = options.join(", ");
            self.errors.push(format!("must be one of: {joined}"));
        }
        self
    }

    /// Value must match the given regex pattern.
    pub fn matches_regex(self, pattern: &str) -> Self {
        match regex::Regex::new(pattern) {
            Ok(re) => {
                if !re.is_match(self.value.as_ref()) {
                    self.errors.push(format!("must match pattern: {pattern}"));
                }
            }
            Err(_) => {
                self.errors
                    .push(format!("invalid regex pattern: {pattern}"));
            }
        }
        self
    }

    /// Custom validation with a predicate and error message.
    pub fn custom(self, predicate: impl FnOnce(&str) -> bool, message: &str) -> Self {
        if !predicate(self.value.as_ref()) {
            self.errors.push(message.to_string());
        }
        self
    }
}

// --- Numeric rules (T: PartialOrd + Display) ---

impl<'a, T: PartialOrd + Display> FieldValidator<'a, T> {
    /// Value must be within the given inclusive range.
    pub fn range(self, range: RangeInclusive<T>) -> Self {
        if self.value < range.start() || self.value > range.end() {
            self.errors.push(format!(
                "must be between {} and {}",
                range.start(),
                range.end()
            ));
        }
        self
    }
}
