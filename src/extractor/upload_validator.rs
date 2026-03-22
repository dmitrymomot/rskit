use crate::extractor::multipart::UploadedFile;

/// Fluent validator for uploaded files.
///
/// Obtained by calling [`UploadedFile::validate()`]. Chain `.max_size()` and
/// `.accept()` calls, then call `.check()` to finalize. All constraint
/// violations are collected before returning.
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
        if self.file.size > max {
            self.errors
                .push(format!("file exceeds maximum size of {}", format_size(max)));
        }
        self
    }

    /// Reject if the content type doesn't match `pattern`.
    ///
    /// Supports exact types (`"image/png"`), wildcard subtypes (`"image/*"`),
    /// and the catch-all `"*/*"`. Parameters after `;` in the content type
    /// are stripped before matching.
    pub fn accept(mut self, pattern: &str) -> Self {
        if !mime_matches(&self.file.content_type, pattern) {
            self.errors.push(format!("file type must match {pattern}"));
        }
        self
    }

    /// Finish validation. Returns `Ok(())` when all rules pass, or a
    /// 422 Unprocessable Entity error with collected messages.
    pub fn check(self) -> crate::error::Result<()> {
        if self.errors.is_empty() {
            Ok(())
        } else {
            let details = serde_json::json!({
                self.file.name.clone(): self.errors,
            });
            Err(
                crate::error::Error::unprocessable_entity("upload validation failed")
                    .with_details(details),
            )
        }
    }
}

/// Check if a content type matches a pattern.
///
/// Parameters after `;` in the content type are stripped before matching.
/// The pattern `"*/*"` matches any type.
fn mime_matches(content_type: &str, pattern: &str) -> bool {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn test_file(name: &str, content_type: &str, size: usize) -> UploadedFile {
        UploadedFile {
            name: name.to_string(),
            content_type: content_type.to_string(),
            size,
            data: bytes::Bytes::from(vec![0u8; size]),
        }
    }

    // -- mime_matches --

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
    fn mime_with_params() {
        assert!(mime_matches("image/png; charset=utf-8", "image/png"));
    }

    #[test]
    fn mime_wildcard_partial_type_rejected() {
        assert!(!mime_matches("imageX/png", "image/*"));
    }

    // -- UploadValidator --

    #[test]
    fn validator_max_size_pass() {
        let f = test_file("f", "application/octet-stream", 5);
        f.validate().max_size(10).check().unwrap();
    }

    #[test]
    fn validator_max_size_fail() {
        let f = test_file("f", "application/octet-stream", 20);
        assert!(f.validate().max_size(10).check().is_err());
    }

    #[test]
    fn validator_max_size_exact_boundary() {
        let f = test_file("f", "application/octet-stream", 10);
        f.validate().max_size(10).check().unwrap();
    }

    #[test]
    fn validator_accept_pass() {
        let f = test_file("f", "image/png", 5);
        f.validate().accept("image/*").check().unwrap();
    }

    #[test]
    fn validator_accept_fail() {
        let f = test_file("f", "text/plain", 5);
        assert!(f.validate().accept("image/*").check().is_err());
    }

    #[test]
    fn validator_chain_both_fail() {
        let f = test_file("f", "text/plain", 20);
        let err = f
            .validate()
            .max_size(10)
            .accept("image/*")
            .check()
            .unwrap_err();
        let details = err.details().expect("expected details");
        let messages = details["f"].as_array().expect("expected array");
        assert_eq!(messages.len(), 2);
    }

    #[test]
    fn validator_chain_both_pass() {
        let f = test_file("f", "image/png", 5);
        f.validate().max_size(10).accept("image/*").check().unwrap();
    }
}
