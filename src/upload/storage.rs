use std::sync::Arc;
use std::time::Duration;

use crate::error::{Error, Result};
use crate::extractor::UploadedFile;

use super::config::BucketConfig;
use super::options::PutOptions;
use super::path::{generate_key, validate_path};

struct StorageInner {
    operator: opendal::Operator,
    public_url: Option<String>,
    max_file_size: Option<usize>,
}

/// S3-compatible file storage backed by OpenDAL.
///
/// Cheaply cloneable (wraps `Arc`). Use `Storage::new()` for production
/// or `Storage::memory()` (behind `upload-test` feature) for testing.
pub struct Storage {
    inner: Arc<StorageInner>,
}

impl Clone for Storage {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl Storage {
    /// Create a new `Storage` from a bucket configuration.
    ///
    /// Validates config and builds an OpenDAL S3 operator. Returns an error
    /// if the configuration is invalid.
    pub fn new(config: &BucketConfig) -> Result<Self> {
        config.validate()?;

        let mut builder = opendal::services::S3::default()
            .bucket(&config.bucket)
            .region(&config.region)
            .endpoint(&config.endpoint);

        if !config.access_key.is_empty() {
            builder = builder.access_key_id(&config.access_key);
        }
        if !config.secret_key.is_empty() {
            builder = builder.secret_access_key(&config.secret_key);
        }

        let operator = opendal::Operator::new(builder)
            .map_err(|e| Error::internal(format!("failed to configure S3 storage: {e}")))?
            .finish();

        let public_url = config.normalized_public_url();
        let max_file_size = config.max_file_size_bytes()?;

        Ok(Self {
            inner: Arc::new(StorageInner {
                operator,
                public_url,
                max_file_size,
            }),
        })
    }

    /// Create an in-memory `Storage` for testing.
    #[cfg(any(test, feature = "upload-test"))]
    pub fn memory() -> Self {
        let operator = opendal::Operator::new(opendal::services::Memory::default())
            .expect("memory operator should never fail")
            .finish();

        Self {
            inner: Arc::new(StorageInner {
                operator,
                public_url: Some("https://test.example.com".to_string()),
                max_file_size: None,
            }),
        }
    }

    /// Upload a file under `prefix/`. Returns the S3 key.
    ///
    /// Validates `max_file_size` if configured. Generates a ULID-based
    /// filename, preserving the original extension.
    pub async fn put(&self, file: &UploadedFile, prefix: &str) -> Result<String> {
        validate_path(prefix)?;

        if let Some(max) = self.inner.max_file_size
            && file.size > max
        {
            return Err(Error::payload_too_large(format!(
                "file size {} exceeds maximum {}",
                file.size, max
            )));
        }

        let ext = file.extension();
        let key = generate_key(prefix, ext.as_deref());

        if let Err(e) = self.inner.operator.write(&key, file.data.clone()).await {
            if let Err(del_err) = self.inner.operator.delete(&key).await {
                tracing::warn!(key = %key, error = %del_err, "failed to clean up partial upload");
            }
            return Err(Error::internal(format!("failed to upload file: {e}")));
        }

        tracing::info!(key = %key, size = file.size, "file uploaded");
        Ok(key)
    }

    /// Upload a file with custom options. Returns the S3 key.
    pub async fn put_with(
        &self,
        file: &UploadedFile,
        prefix: &str,
        opts: PutOptions,
    ) -> Result<String> {
        validate_path(prefix)?;

        if let Some(max) = self.inner.max_file_size
            && file.size > max
        {
            return Err(Error::payload_too_large(format!(
                "file size {} exceeds maximum {}",
                file.size, max
            )));
        }

        let ext = file.extension();
        let key = generate_key(prefix, ext.as_deref());

        let content_type = opts.content_type.as_deref().unwrap_or(&file.content_type);

        let mut write_op = self
            .inner
            .operator
            .write_with(&key, file.data.clone())
            .content_type(content_type);

        if let Some(ref cd) = opts.content_disposition {
            write_op = write_op.content_disposition(cd);
        }
        if let Some(ref cc) = opts.cache_control {
            write_op = write_op.cache_control(cc);
        }

        if let Err(e) = write_op.await {
            if let Err(del_err) = self.inner.operator.delete(&key).await {
                tracing::warn!(key = %key, error = %del_err, "failed to clean up partial upload");
            }
            return Err(Error::internal(format!("failed to upload file: {e}")));
        }

        tracing::info!(key = %key, size = file.size, "file uploaded");
        Ok(key)
    }

    /// Delete a single object by key.
    ///
    /// Deleting a non-existent key is a no-op (returns `Ok(())`).
    pub async fn delete(&self, key: &str) -> Result<()> {
        validate_path(key)?;
        self.inner
            .operator
            .delete(key)
            .await
            .map_err(|e| Error::internal(format!("failed to delete file: {e}")))?;
        tracing::info!(key = %key, "file deleted");
        Ok(())
    }

    /// Delete all objects under a prefix.
    ///
    /// Uses OpenDAL's `remove_all()`. O(n) network calls.
    pub async fn delete_prefix(&self, prefix: &str) -> Result<()> {
        validate_path(prefix)?;
        self.inner
            .operator
            .remove_all(prefix)
            .await
            .map_err(|e| Error::internal(format!("failed to delete prefix: {e}")))?;
        tracing::info!(prefix = %prefix, "prefix deleted");
        Ok(())
    }

    /// Public URL (string concatenation, no network call).
    ///
    /// Returns an error if `public_url` is not configured.
    pub fn url(&self, key: &str) -> Result<String> {
        validate_path(key)?;
        let base = self
            .inner
            .public_url
            .as_ref()
            .ok_or_else(|| Error::internal("public_url not configured"))?;
        Ok(format!("{base}/{key}"))
    }

    /// Presigned URL via OpenDAL `presign_read()`.
    ///
    /// Works with any S3-compatible service. May error on backends that
    /// don't support presigning (e.g. Memory in tests).
    pub async fn presigned_url(&self, key: &str, expires_in: Duration) -> Result<String> {
        validate_path(key)?;
        let req = self
            .inner
            .operator
            .presign_read(key, expires_in)
            .await
            .map_err(|e| Error::internal(format!("failed to generate presigned URL: {e}")))?;
        Ok(req.uri().to_string())
    }

    /// Check if a key exists.
    pub async fn exists(&self, key: &str) -> Result<bool> {
        validate_path(key)?;
        self.inner
            .operator
            .exists(key)
            .await
            .map_err(|e| Error::internal(format!("failed to check file existence: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;

    fn test_file(name: &str, content_type: &str, data: &[u8]) -> UploadedFile {
        UploadedFile {
            name: name.to_string(),
            content_type: content_type.to_string(),
            size: data.len(),
            data: Bytes::copy_from_slice(data),
        }
    }

    #[tokio::test]
    async fn put_returns_key_with_prefix_and_extension() {
        let storage = Storage::memory();
        let file = test_file("photo.jpg", "image/jpeg", b"imgdata");
        let key = storage.put(&file, "avatars/").await.unwrap();
        assert!(key.starts_with("avatars/"));
        assert!(key.ends_with(".jpg"));
    }

    #[tokio::test]
    async fn put_file_exists_after_upload() {
        let storage = Storage::memory();
        let file = test_file("doc.pdf", "application/pdf", b"pdf content");
        let key = storage.put(&file, "docs/").await.unwrap();
        assert!(storage.exists(&key).await.unwrap());
    }

    #[tokio::test]
    async fn put_respects_max_file_size() {
        let operator = opendal::Operator::new(opendal::services::Memory::default())
            .unwrap()
            .finish();
        let config = BucketConfig {
            bucket: "test".into(),
            endpoint: "https://s3.example.com".into(),
            max_file_size: Some("5b".into()),
            ..Default::default()
        };
        let storage = Storage {
            inner: Arc::new(StorageInner {
                operator,
                public_url: None,
                max_file_size: config.max_file_size_bytes().unwrap(),
            }),
        };

        let file = test_file("big.bin", "application/octet-stream", &[0u8; 10]);
        let err = storage.put(&file, "uploads/").await.unwrap_err();
        assert_eq!(err.status(), http::StatusCode::PAYLOAD_TOO_LARGE);
    }

    #[tokio::test]
    async fn put_with_options() {
        let storage = Storage::memory();
        let file = test_file("report.pdf", "application/pdf", b"pdf");
        let key = storage
            .put_with(
                &file,
                "reports/",
                PutOptions {
                    content_disposition: Some("attachment".into()),
                    cache_control: Some("max-age=3600".into()),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        assert!(storage.exists(&key).await.unwrap());
    }

    #[tokio::test]
    async fn delete_removes_file() {
        let storage = Storage::memory();
        let file = test_file("a.txt", "text/plain", b"hello");
        let key = storage.put(&file, "tmp/").await.unwrap();
        assert!(storage.exists(&key).await.unwrap());

        storage.delete(&key).await.unwrap();
        assert!(!storage.exists(&key).await.unwrap());
    }

    #[tokio::test]
    async fn delete_nonexistent_key_is_noop() {
        let storage = Storage::memory();
        // Should not error
        storage.delete("nonexistent/file.txt").await.unwrap();
    }

    #[tokio::test]
    async fn delete_prefix_removes_all() {
        let storage = Storage::memory();
        let f1 = test_file("a.txt", "text/plain", b"a");
        let f2 = test_file("b.txt", "text/plain", b"b");
        let k1 = storage.put(&f1, "prefix/").await.unwrap();
        let k2 = storage.put(&f2, "prefix/").await.unwrap();

        storage.delete_prefix("prefix/").await.unwrap();

        assert!(!storage.exists(&k1).await.unwrap());
        assert!(!storage.exists(&k2).await.unwrap());
    }

    #[tokio::test]
    async fn url_returns_public_url() {
        let storage = Storage::memory();
        let url = storage.url("avatars/photo.jpg").unwrap();
        assert_eq!(url, "https://test.example.com/avatars/photo.jpg");
    }

    #[tokio::test]
    async fn url_errors_without_public_url() {
        let operator = opendal::Operator::new(opendal::services::Memory::default())
            .unwrap()
            .finish();
        let storage = Storage {
            inner: Arc::new(StorageInner {
                operator,
                public_url: None,
                max_file_size: None,
            }),
        };
        assert!(storage.url("key.jpg").is_err());
    }

    #[tokio::test]
    async fn presigned_url_errors_on_memory_backend() {
        let storage = Storage::memory();
        let result = storage
            .presigned_url("key.jpg", Duration::from_secs(3600))
            .await;
        // Memory backend does not support presigning
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn exists_false_for_missing_key() {
        let storage = Storage::memory();
        assert!(!storage.exists("nonexistent.jpg").await.unwrap());
    }

    #[tokio::test]
    async fn put_rejects_path_traversal() {
        let storage = Storage::memory();
        let file = test_file("f.txt", "text/plain", b"data");
        assert!(storage.put(&file, "../etc/").await.is_err());
    }

    #[tokio::test]
    async fn put_rejects_absolute_path() {
        let storage = Storage::memory();
        let file = test_file("f.txt", "text/plain", b"data");
        assert!(storage.put(&file, "/root/").await.is_err());
    }

    #[tokio::test]
    async fn put_rejects_empty_prefix() {
        let storage = Storage::memory();
        let file = test_file("f.txt", "text/plain", b"data");
        assert!(storage.put(&file, "").await.is_err());
    }
}
