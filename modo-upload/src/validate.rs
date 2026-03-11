use crate::file::UploadedFile;

/// Fluent validator for uploaded files.
pub struct UploadValidator<'a> {
    file: &'a UploadedFile,
    errors: Vec<String>,
}

impl<'a> UploadValidator<'a> {
    pub(crate) fn new(file: &'a UploadedFile) -> Self {
        Self {
            file,
            errors: Vec::new(),
        }
    }

    /// Reject if the file exceeds `max` bytes.
    pub fn max_size(mut self, max: usize) -> Self {
        if self.file.size() > max {
            self.errors
                .push(format!("File exceeds maximum size of {}", format_size(max)));
        }
        self
    }

    /// Reject if the content type doesn't match `pattern`.
    /// Supports exact types (`image/png`) and wildcard subtypes (`image/*`).
    pub fn accept(mut self, pattern: &str) -> Self {
        if !mime_matches(self.file.content_type(), pattern) {
            self.errors.push(format!("File type must match {pattern}"));
        }
        self
    }

    /// Finish validation. Returns `Ok(())` or a validation error.
    pub fn check(self) -> Result<(), modo::Error> {
        if self.errors.is_empty() {
            Ok(())
        } else {
            Err(modo::validate::validation_error(vec![(
                self.file.name(),
                self.errors,
            )]))
        }
    }
}

/// Check if a content type matches a pattern (e.g. `image/*` matches `image/png`).
/// Parameters after `;` in the content type are stripped before matching.
pub fn mime_matches(content_type: &str, pattern: &str) -> bool {
    let content_type = content_type
        .split(';')
        .next()
        .unwrap_or(content_type)
        .trim();
    if pattern == "*/*" {
        return true;
    }
    if let Some(prefix) = pattern.strip_suffix("/*") {
        content_type.starts_with(prefix)
            && content_type
                .as_bytes()
                .get(prefix.len())
                .is_some_and(|&b| b == b'/')
    } else {
        content_type == pattern
    }
}

fn format_size(bytes: usize) -> String {
    if bytes >= 1024 * 1024 * 1024 && bytes.is_multiple_of(1024 * 1024 * 1024) {
        format!("{}GB", bytes / (1024 * 1024 * 1024))
    } else if bytes >= 1024 * 1024 && bytes.is_multiple_of(1024 * 1024) {
        format!("{}MB", bytes / (1024 * 1024))
    } else if bytes >= 1024 && bytes.is_multiple_of(1024) {
        format!("{}KB", bytes / 1024)
    } else {
        format!("{bytes}B")
    }
}

/// Convert megabytes to bytes.
pub fn mb(n: usize) -> usize {
    n * 1024 * 1024
}

/// Convert kilobytes to bytes.
pub fn kb(n: usize) -> usize {
    n * 1024
}

/// Convert gigabytes to bytes.
pub fn gb(n: usize) -> usize {
    n * 1024 * 1024 * 1024
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mime_exact_match() {
        assert!(mime_matches("image/png", "image/png"));
        assert!(!mime_matches("image/jpeg", "image/png"));
    }

    #[test]
    fn mime_wildcard_match() {
        assert!(mime_matches("image/png", "image/*"));
        assert!(mime_matches("image/jpeg", "image/*"));
        assert!(!mime_matches("text/plain", "image/*"));
    }

    #[test]
    fn mime_any_match() {
        assert!(mime_matches("anything/here", "*/*"));
    }

    #[test]
    fn mime_with_params_exact() {
        assert!(mime_matches("image/png; charset=utf-8", "image/png"));
        assert!(!mime_matches("image/jpeg; charset=utf-8", "image/png"));
    }

    #[test]
    fn mime_with_params_wildcard() {
        assert!(mime_matches("image/png; charset=utf-8", "image/*"));
        assert!(!mime_matches("text/plain; charset=utf-8", "image/*"));
    }

    #[test]
    fn mime_empty_content_type() {
        assert!(!mime_matches("", "image/png"));
        assert!(!mime_matches("image/png", ""));
    }

    #[test]
    fn size_helpers() {
        assert_eq!(kb(1), 1024);
        assert_eq!(mb(1), 1024 * 1024);
        assert_eq!(gb(1), 1024 * 1024 * 1024);
        assert_eq!(mb(5), 5 * 1024 * 1024);
    }

    #[test]
    fn format_size_display() {
        assert_eq!(format_size(500), "500B");
        assert_eq!(format_size(1024), "1KB");
        assert_eq!(format_size(5 * 1024 * 1024), "5MB");
        assert_eq!(format_size(2 * 1024 * 1024 * 1024), "2GB");
    }

    #[test]
    fn format_size_non_aligned_falls_back_to_bytes() {
        assert_eq!(format_size(2047), "2047B");
        assert_eq!(format_size(1025), "1025B");
        assert_eq!(format_size(1024 * 1024 + 1), "1048577B");
    }

    // -- UploadValidator --

    #[test]
    fn validator_max_size_pass() {
        let f = UploadedFile::__test_new("f", "a.bin", "application/octet-stream", &[0u8; 5]);
        f.validate().max_size(10).check().unwrap();
    }

    #[test]
    fn validator_max_size_fail() {
        let f = UploadedFile::__test_new("f", "a.bin", "application/octet-stream", &[0u8; 20]);
        assert!(f.validate().max_size(10).check().is_err());
    }

    #[test]
    fn validator_max_size_exact_boundary() {
        let f = UploadedFile::__test_new("f", "a.bin", "application/octet-stream", &[0u8; 10]);
        // size == max should pass (not >)
        f.validate().max_size(10).check().unwrap();
    }

    #[test]
    fn validator_accept_pass() {
        let f = UploadedFile::__test_new("f", "img.png", "image/png", b"img");
        f.validate().accept("image/*").check().unwrap();
    }

    #[test]
    fn validator_accept_fail() {
        let f = UploadedFile::__test_new("f", "doc.txt", "text/plain", b"text");
        assert!(f.validate().accept("image/*").check().is_err());
    }

    #[test]
    fn validator_chain_both_fail() {
        let f = UploadedFile::__test_new("f", "doc.txt", "text/plain", &[0u8; 20]);
        let err = f
            .validate()
            .max_size(10)
            .accept("image/*")
            .check()
            .unwrap_err();
        // Both errors should be collected
        let details = err.details();
        let messages = details
            .get("f")
            .expect("expected details for field 'f'")
            .as_array()
            .expect("expected JSON array");
        assert_eq!(
            messages.len(),
            2,
            "expected 2 validation messages, got: {messages:?}"
        );
    }

    #[test]
    fn validator_chain_both_pass() {
        let f = UploadedFile::__test_new("f", "img.png", "image/png", &[0u8; 5]);
        f.validate().max_size(10).accept("image/*").check().unwrap();
    }

    #[test]
    fn mime_semicolon_no_params() {
        assert!(mime_matches("image/png;", "image/png"));
    }

    #[test]
    fn mime_case_sensitive() {
        assert!(!mime_matches("Image/PNG", "image/png"));
    }

    #[test]
    fn mime_wildcard_invalid_form() {
        assert!(!mime_matches("image/png", "*/image"));
    }

    #[test]
    fn mime_leading_trailing_whitespace() {
        assert!(mime_matches(" image/png ", "image/png"));
    }

    #[test]
    fn mime_wildcard_partial_type_rejected() {
        assert!(!mime_matches("imageX/png", "image/*"));
    }

    #[test]
    fn format_size_zero() {
        assert_eq!(format_size(0), "0B");
    }

    #[test]
    fn format_size_boundary_1023() {
        assert_eq!(format_size(1023), "1023B");
    }

    #[test]
    fn format_size_boundary_below_mb() {
        assert_eq!(format_size(1024 * 1024 - 1), "1048575B");
    }
}
