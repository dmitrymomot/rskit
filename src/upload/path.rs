use crate::error::{Error, Result};

/// Validate a storage path (prefix or key).
///
/// Rejects path traversal (`..`), absolute paths (`/`), empty strings,
/// and control characters.
pub(crate) fn validate_path(path: &str) -> Result<()> {
    if path.is_empty() {
        return Err(Error::bad_request("storage path must not be empty"));
    }
    if path.starts_with('/') {
        return Err(Error::bad_request("storage path must not start with '/'"));
    }
    if path.split('/').any(|seg| seg == "..") {
        return Err(Error::bad_request(
            "storage path must not contain '..' segments",
        ));
    }
    if path.chars().any(|c| c.is_control()) {
        return Err(Error::bad_request(
            "storage path must not contain control characters",
        ));
    }
    Ok(())
}

/// Generate a unique storage key for an uploaded file.
///
/// Format: `{prefix}{ulid}.{ext}` or `{prefix}{ulid}` if no extension.
pub(crate) fn generate_key(prefix: &str, extension: Option<&str>) -> String {
    let id = crate::id::ulid();
    match extension {
        Some(ext) if !ext.is_empty() => format!("{prefix}{id}.{ext}"),
        _ => format!("{prefix}{id}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- validate_path --

    #[test]
    fn valid_prefix() {
        validate_path("avatars/").unwrap();
    }

    #[test]
    fn valid_nested_prefix() {
        validate_path("uploads/images/2024/").unwrap();
    }

    #[test]
    fn valid_key() {
        validate_path("avatars/01ABC.jpg").unwrap();
    }

    #[test]
    fn rejects_empty() {
        assert!(validate_path("").is_err());
    }

    #[test]
    fn rejects_leading_slash() {
        assert!(validate_path("/avatars/").is_err());
    }

    #[test]
    fn rejects_dot_dot() {
        assert!(validate_path("avatars/../secrets/").is_err());
    }

    #[test]
    fn rejects_dot_dot_at_start() {
        assert!(validate_path("../etc/passwd").is_err());
    }

    #[test]
    fn allows_dots_in_filename() {
        validate_path("archive.tar.gz").unwrap();
    }

    #[test]
    fn rejects_control_chars() {
        assert!(validate_path("avatars/\x00file.jpg").is_err());
        assert!(validate_path("avatars/\nfile.jpg").is_err());
    }

    // -- generate_key --

    #[test]
    fn generate_key_with_extension() {
        let key = generate_key("avatars/", Some("jpg"));
        assert!(key.starts_with("avatars/"));
        assert!(key.ends_with(".jpg"));
        // ULID is 26 chars: "avatars/" (8) + 26 + ".jpg" (4) = 38
        assert_eq!(key.len(), 38);
    }

    #[test]
    fn generate_key_without_extension() {
        let key = generate_key("docs/", None);
        assert!(key.starts_with("docs/"));
        assert!(!key.contains('.'));
        // "docs/" (5) + 26 = 31
        assert_eq!(key.len(), 31);
    }

    #[test]
    fn generate_key_empty_extension() {
        let key = generate_key("docs/", Some(""));
        assert!(!key.contains('.'));
    }

    #[test]
    fn generate_key_unique() {
        let key1 = generate_key("a/", Some("txt"));
        let key2 = generate_key("a/", Some("txt"));
        assert_ne!(key1, key2);
    }
}
