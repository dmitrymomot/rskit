#[cfg(feature = "local")]
pub mod local;
#[cfg(feature = "opendal")]
pub mod opendal;

use crate::file::UploadedFile;
use crate::stream::BufferedUpload;
use std::path::{Component, Path, PathBuf};

/// Metadata for a stored file.
pub struct StoredFile {
    /// Relative path within the storage backend (e.g. `avatars/01HXK3Q1A2B3.jpg`).
    pub path: String,
    /// File size in bytes.
    pub size: u64,
}

/// Trait for persisting uploaded files to a storage backend.
#[async_trait::async_trait]
pub trait FileStorage: Send + Sync + 'static {
    /// Store a buffered file under `prefix/`. Returns the stored path and size.
    async fn store(&self, prefix: &str, file: &UploadedFile) -> Result<StoredFile, modo::Error>;

    /// Store a buffered-chunked file under `prefix/`. Returns the stored path and size.
    async fn store_stream(
        &self,
        prefix: &str,
        stream: &mut BufferedUpload,
    ) -> Result<StoredFile, modo::Error>;

    /// Delete a file by its storage path.
    async fn delete(&self, path: &str) -> Result<(), modo::Error>;

    /// Check if a file exists at the given storage path.
    async fn exists(&self, path: &str) -> Result<bool, modo::Error>;
}

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

/// Create a [`FileStorage`] backend from the given [`UploadConfig`](crate::config::UploadConfig).
///
/// The appropriate implementation is selected based on `config.backend`:
/// - `Local` — requires the `local` feature
/// - `S3` — requires the `opendal` feature
pub fn storage(config: &crate::config::UploadConfig) -> Result<Box<dyn FileStorage>, modo::Error> {
    match config.backend {
        #[cfg(feature = "local")]
        crate::config::StorageBackend::Local => {
            Ok(Box::new(local::LocalStorage::new(&config.path)))
        }
        #[cfg(not(feature = "local"))]
        crate::config::StorageBackend::Local => Err(modo::Error::internal(
            "Local storage backend requires the `local` feature",
        )),

        #[cfg(feature = "opendal")]
        crate::config::StorageBackend::S3 => {
            let s3 = &config.s3;
            let mut builder = ::opendal::services::S3::default()
                .bucket(&s3.bucket)
                .region(&s3.region);
            if !s3.endpoint.is_empty() {
                builder = builder.endpoint(&s3.endpoint);
            }
            if !s3.access_key_id.is_empty() {
                builder = builder.access_key_id(&s3.access_key_id);
            }
            if !s3.secret_access_key.is_empty() {
                builder = builder.secret_access_key(&s3.secret_access_key);
            }
            let op = ::opendal::Operator::new(builder)
                .map_err(|e| modo::Error::internal(format!("Failed to configure S3 storage: {e}")))?
                .finish();
            Ok(Box::new(self::opendal::OpendalStorage::new(op)))
        }
        #[cfg(not(feature = "opendal"))]
        crate::config::StorageBackend::S3 => Err(modo::Error::internal(
            "S3 storage backend requires the `opendal` feature",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

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
        use super::super::validate_logical_path;

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
