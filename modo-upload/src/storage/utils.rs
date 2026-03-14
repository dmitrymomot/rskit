use std::path::{Component, Path, PathBuf};

/// Validate that `path` stays within `base` by rejecting `..`, absolute paths, and other
/// non-normal components. Returns the resolved path under `base`.
pub(crate) fn ensure_within(base: &Path, path: &Path) -> Result<PathBuf, modo::Error> {
    let mut result = base.to_path_buf();
    for component in path.components() {
        match component {
            Component::Normal(c) => result.push(c),
            // `.` is harmless in filesystem paths — silently stripped.
            // (Object-store keys must be canonical, so `validate_logical_path` rejects `.`.)
            Component::CurDir => {}
            _ => return Err(modo::Error::internal("Invalid storage path")),
        }
    }
    Ok(result)
}

/// Validate that a logical path (for object stores) contains no `..` or leading `/`.
#[cfg(feature = "opendal")]
pub(crate) fn validate_logical_path(path: &str) -> Result<(), modo::Error> {
    if path.starts_with('/') {
        return Err(modo::Error::internal("Invalid storage path"));
    }
    for segment in path.split('/') {
        if segment == ".." || segment == "." {
            return Err(modo::Error::internal("Invalid storage path"));
        }
    }
    Ok(())
}

/// Generate a unique filename: `{ulid}.{ext}`.
pub(crate) fn generate_filename(original: &str) -> String {
    let id = ulid::Ulid::new().to_string().to_lowercase();
    match crate::file::extract_extension(original) {
        Some(ext) => format!("{id}.{}", ext.to_ascii_lowercase()),
        None => id,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- ensure_within --

    #[test]
    fn ensure_within_normal_path() {
        let result = ensure_within(Path::new("base"), Path::new("sub/file.txt")).unwrap();
        assert_eq!(result, PathBuf::from("base/sub/file.txt"));
    }

    #[test]
    fn ensure_within_curdir_stripped() {
        let result = ensure_within(Path::new("base"), Path::new("./sub/file.txt")).unwrap();
        assert_eq!(result, PathBuf::from("base/sub/file.txt"));
    }

    #[test]
    fn ensure_within_rejects_parent() {
        let result = ensure_within(Path::new("base"), Path::new("../escape"));
        assert!(result.is_err());
    }

    #[test]
    fn ensure_within_rejects_absolute() {
        let result = ensure_within(Path::new("base"), Path::new("/etc/passwd"));
        assert!(result.is_err());
    }

    #[test]
    fn ensure_within_empty_path() {
        let result = ensure_within(Path::new("base"), Path::new("")).unwrap();
        assert_eq!(result, PathBuf::from("base"));
    }

    // -- generate_filename --

    #[test]
    fn generate_filename_with_ext() {
        let name = generate_filename("photo.JPG");
        assert!(name.ends_with(".jpg"), "expected .jpg suffix, got: {name}");
        // ULID is 26 chars + dot + extension
        assert!(name.len() > 26);
    }

    #[test]
    fn generate_filename_without_ext() {
        let name = generate_filename("noext");
        assert!(!name.contains('.'), "expected no dot, got: {name}");
        assert_eq!(name.len(), 26); // lowercase ULID
    }

    #[test]
    fn generate_filename_compound_ext() {
        let name = generate_filename("archive.tar.gz");
        assert!(name.ends_with(".gz"), "expected .gz suffix, got: {name}");
    }

    #[test]
    fn generate_filename_unique() {
        let a = generate_filename("test.txt");
        let b = generate_filename("test.txt");
        assert_ne!(a, b);
    }

    // -- validate_logical_path (opendal only) --

    #[cfg(feature = "opendal")]
    mod opendal_tests {
        use super::validate_logical_path;

        #[test]
        fn validate_logical_path_ok() {
            assert!(validate_logical_path("prefix/file.txt").is_ok());
        }

        #[test]
        fn validate_logical_path_rejects_leading_slash() {
            assert!(validate_logical_path("/absolute").is_err());
        }

        #[test]
        fn validate_logical_path_rejects_dotdot() {
            assert!(validate_logical_path("a/../escape").is_err());
        }

        #[test]
        fn validate_logical_path_rejects_dot() {
            assert!(validate_logical_path("a/./b").is_err());
        }
    }
}
