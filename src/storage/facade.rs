use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;

use crate::error::{Error, Result};

use super::backend::BackendKind;
use super::client::RemoteBackend;
use super::config::BucketConfig;
use super::fetch::fetch_url;
use super::options::PutOptions;
use super::path::{generate_key, validate_path};

#[cfg(any(test, feature = "storage-test"))]
use super::memory::MemoryBackend;

/// Input for `Storage::put()` and `Storage::put_with()`.
///
/// Use `PutInput::from_upload()` to build from an `UploadedFile` received
/// via multipart form data.
pub struct PutInput {
    /// Raw file bytes.
    pub data: Bytes,
    /// Storage prefix (e.g., `"avatars/"`).
    pub prefix: String,
    /// Original filename — used to extract extension. `None` produces extensionless keys.
    pub filename: Option<String>,
    /// MIME content type (e.g., `"image/jpeg"`).
    pub content_type: String,
}

impl PutInput {
    /// Extract file extension from `filename`, if present.
    fn extension(&self) -> Option<String> {
        let name = self.filename.as_deref()?;
        if name.is_empty() {
            return None;
        }
        let ext = name.rsplit('.').next()?;
        if ext == name {
            None
        } else {
            Some(ext.to_ascii_lowercase())
        }
    }
}

/// Input for `Storage::put_from_url()` and `Storage::put_from_url_with()`.
pub struct PutFromUrlInput {
    /// Source URL to fetch from (must be http or https).
    pub url: String,
    /// Storage prefix (e.g., `"avatars/"`).
    pub prefix: String,
    /// Optional filename hint — used to extract extension. `None` produces extensionless keys.
    pub filename: Option<String>,
}

pub(crate) struct StorageInner {
    pub(crate) backend: BackendKind,
    pub(crate) public_url: Option<String>,
    pub(crate) max_file_size: Option<usize>,
}

/// S3-compatible file storage.
///
/// Cheaply cloneable (wraps `Arc`). Use `Storage::new()` to create a production
/// instance from a `BucketConfig`. `Storage::memory()` is available inside
/// `#[cfg(test)]` blocks and when the `storage-test` feature is enabled.
pub struct Storage {
    pub(crate) inner: Arc<StorageInner>,
}

impl Clone for Storage {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl Storage {
    /// Create from a bucket configuration (builds RemoteBackend).
    pub fn new(config: &BucketConfig) -> Result<Self> {
        config.validate()?;

        let region = config
            .region
            .clone()
            .unwrap_or_else(|| "us-east-1".to_string());
        let backend = RemoteBackend::new(
            config.bucket.clone(),
            config.endpoint.clone(),
            config.access_key.clone(),
            config.secret_key.clone(),
            region,
            config.path_style,
        )?;

        Ok(Self {
            inner: Arc::new(StorageInner {
                backend: BackendKind::Remote(backend),
                public_url: config.normalized_public_url(),
                max_file_size: config.max_file_size_bytes()?,
            }),
        })
    }

    /// In-memory storage for testing.
    ///
    /// Available inside `#[cfg(test)]` blocks without any extra feature, and
    /// also when the `storage-test` feature is enabled (for integration tests).
    #[cfg(any(test, feature = "storage-test"))]
    pub fn memory() -> Self {
        Self {
            inner: Arc::new(StorageInner {
                backend: BackendKind::Memory(MemoryBackend::new()),
                public_url: Some("https://test.example.com".to_string()),
                max_file_size: None,
            }),
        }
    }

    /// Upload bytes. Returns the generated S3 key.
    pub async fn put(&self, input: &PutInput) -> Result<String> {
        self.put_inner(input, &PutOptions::default()).await
    }

    /// Upload bytes with custom options. Returns the generated S3 key.
    pub async fn put_with(&self, input: &PutInput, opts: PutOptions) -> Result<String> {
        self.put_inner(input, &opts).await
    }

    async fn put_inner(&self, input: &PutInput, opts: &PutOptions) -> Result<String> {
        validate_path(&input.prefix)?;

        if let Some(max) = self.inner.max_file_size
            && input.data.len() > max
        {
            return Err(Error::payload_too_large(format!(
                "file size {} exceeds maximum {}",
                input.data.len(),
                max
            )));
        }

        let ext = input.extension();
        let key = generate_key(&input.prefix, ext.as_deref());

        let content_type = opts.content_type.as_deref().unwrap_or(&input.content_type);

        let result = match &self.inner.backend {
            BackendKind::Remote(b) => b.put(&key, input.data.clone(), content_type, opts).await,
            BackendKind::Memory(b) => b.put(&key, input.data.clone(), content_type, opts).await,
        };

        if let Err(e) = result {
            let delete_result = match &self.inner.backend {
                BackendKind::Remote(b) => b.delete(&key).await,
                BackendKind::Memory(b) => b.delete(&key).await,
            };
            if let Err(del_err) = delete_result {
                tracing::warn!(key = %key, error = %del_err, "failed to clean up partial upload");
            }
            return Err(e);
        }

        tracing::info!(key = %key, size = input.data.len(), "file uploaded");
        Ok(key)
    }

    /// Delete a single key. No-op if missing.
    pub async fn delete(&self, key: &str) -> Result<()> {
        validate_path(key)?;
        match &self.inner.backend {
            BackendKind::Remote(b) => b.delete(key).await,
            BackendKind::Memory(b) => b.delete(key).await,
        }
        .map_err(|e| Error::internal(format!("failed to delete file: {e}")))?;
        tracing::info!(key = %key, "file deleted");
        Ok(())
    }

    /// Delete all keys under prefix. O(n) network calls.
    pub async fn delete_prefix(&self, prefix: &str) -> Result<()> {
        validate_path(prefix)?;
        let keys = match &self.inner.backend {
            BackendKind::Remote(b) => b.list(prefix).await,
            BackendKind::Memory(b) => b.list(prefix).await,
        }
        .map_err(|e| Error::internal(format!("failed to list prefix: {e}")))?;

        for key in &keys {
            match &self.inner.backend {
                BackendKind::Remote(b) => b.delete(key).await,
                BackendKind::Memory(b) => b.delete(key).await,
            }
            .map_err(|e| Error::internal(format!("failed to delete {key}: {e}")))?;
        }

        tracing::info!(prefix = %prefix, count = keys.len(), "prefix deleted");
        Ok(())
    }

    /// Public URL (string concatenation, no network call).
    ///
    /// Requires `public_url` to be set in `BucketConfig`. Returns an error if
    /// `public_url` is not configured.
    pub fn url(&self, key: &str) -> Result<String> {
        validate_path(key)?;
        let base = self
            .inner
            .public_url
            .as_ref()
            .ok_or_else(|| Error::internal("public_url not configured"))?;
        Ok(format!("{base}/{key}"))
    }

    /// Presigned GET URL with expiry.
    pub async fn presigned_url(&self, key: &str, expires_in: Duration) -> Result<String> {
        validate_path(key)?;
        match &self.inner.backend {
            BackendKind::Remote(b) => b.presigned_url(key, expires_in).await,
            BackendKind::Memory(b) => b.presigned_url(key, expires_in).await,
        }
        .map_err(|e| Error::internal(format!("failed to generate presigned URL: {e}")))
    }

    /// Check if a key exists.
    pub async fn exists(&self, key: &str) -> Result<bool> {
        validate_path(key)?;
        match &self.inner.backend {
            BackendKind::Remote(b) => b.exists(key).await,
            BackendKind::Memory(b) => b.exists(key).await,
        }
        .map_err(|e| Error::internal(format!("failed to check existence: {e}")))
    }

    /// Fetch a file from a URL and upload it. Returns the generated S3 key.
    ///
    /// Redirects are not followed. A hard-coded 30-second timeout applies.
    /// Returns an error when called on the memory backend.
    pub async fn put_from_url(&self, input: &PutFromUrlInput) -> Result<String> {
        self.put_from_url_inner(input, &PutOptions::default()).await
    }

    /// Fetch a file from a URL and upload it with custom options. Returns the generated S3 key.
    ///
    /// Redirects are not followed. A hard-coded 30-second timeout applies.
    /// Returns an error when called on the memory backend.
    pub async fn put_from_url_with(
        &self,
        input: &PutFromUrlInput,
        opts: PutOptions,
    ) -> Result<String> {
        self.put_from_url_inner(input, &opts).await
    }

    async fn put_from_url_inner(
        &self,
        input: &PutFromUrlInput,
        opts: &PutOptions,
    ) -> Result<String> {
        let client = self.inner.backend.http_client()?;
        let fetched = fetch_url(client, &input.url, self.inner.max_file_size).await?;

        let put_input = PutInput {
            data: fetched.data,
            prefix: input.prefix.clone(),
            filename: input.filename.clone(),
            content_type: fetched.content_type,
        };

        self.put_inner(&put_input, opts).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;

    #[tokio::test]
    async fn put_returns_key_with_prefix_and_extension() {
        let storage = Storage::memory();
        let input = PutInput {
            data: Bytes::from("imgdata"),
            prefix: "avatars/".into(),
            filename: Some("photo.jpg".into()),
            content_type: "image/jpeg".into(),
        };
        let key = storage.put(&input).await.unwrap();
        assert!(key.starts_with("avatars/"));
        assert!(key.ends_with(".jpg"));
    }

    #[tokio::test]
    async fn put_no_extension_without_filename() {
        let storage = Storage::memory();
        let input = PutInput {
            data: Bytes::from("data"),
            prefix: "raw/".into(),
            filename: None,
            content_type: "application/octet-stream".into(),
        };
        let key = storage.put(&input).await.unwrap();
        assert!(key.starts_with("raw/"));
        assert!(!key.contains('.'));
    }

    #[tokio::test]
    async fn put_no_extension_with_empty_filename() {
        let storage = Storage::memory();
        let input = PutInput {
            data: Bytes::from("data"),
            prefix: "raw/".into(),
            filename: Some("".into()),
            content_type: "application/octet-stream".into(),
        };
        let key = storage.put(&input).await.unwrap();
        assert!(!key.contains('.'));
    }

    #[tokio::test]
    async fn put_file_exists_after_upload() {
        let storage = Storage::memory();
        let input = PutInput {
            data: Bytes::from("pdf content"),
            prefix: "docs/".into(),
            filename: Some("doc.pdf".into()),
            content_type: "application/pdf".into(),
        };
        let key = storage.put(&input).await.unwrap();
        assert!(storage.exists(&key).await.unwrap());
    }

    #[tokio::test]
    async fn put_respects_max_file_size() {
        let storage = Storage {
            inner: Arc::new(StorageInner {
                backend: BackendKind::Memory(MemoryBackend::new()),
                public_url: None,
                max_file_size: Some(5),
            }),
        };
        let input = PutInput {
            data: Bytes::from(vec![0u8; 10]),
            prefix: "uploads/".into(),
            filename: Some("big.bin".into()),
            content_type: "application/octet-stream".into(),
        };
        let err = storage.put(&input).await.err().unwrap();
        assert_eq!(err.status(), http::StatusCode::PAYLOAD_TOO_LARGE);
    }

    #[tokio::test]
    async fn put_with_options() {
        let storage = Storage::memory();
        let input = PutInput {
            data: Bytes::from("pdf"),
            prefix: "reports/".into(),
            filename: Some("report.pdf".into()),
            content_type: "application/pdf".into(),
        };
        let key = storage
            .put_with(
                &input,
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
        let input = PutInput {
            data: Bytes::from("hello"),
            prefix: "tmp/".into(),
            filename: Some("a.txt".into()),
            content_type: "text/plain".into(),
        };
        let key = storage.put(&input).await.unwrap();
        storage.delete(&key).await.unwrap();
        assert!(!storage.exists(&key).await.unwrap());
    }

    #[tokio::test]
    async fn delete_nonexistent_is_noop() {
        let storage = Storage::memory();
        storage.delete("nonexistent/file.txt").await.unwrap();
    }

    #[tokio::test]
    async fn delete_prefix_removes_all() {
        let storage = Storage::memory();
        let f1 = PutInput {
            data: Bytes::from("a"),
            prefix: "prefix/".into(),
            filename: Some("a.txt".into()),
            content_type: "text/plain".into(),
        };
        let f2 = PutInput {
            data: Bytes::from("b"),
            prefix: "prefix/".into(),
            filename: Some("b.txt".into()),
            content_type: "text/plain".into(),
        };
        let k1 = storage.put(&f1).await.unwrap();
        let k2 = storage.put(&f2).await.unwrap();

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
        let storage = Storage {
            inner: Arc::new(StorageInner {
                backend: BackendKind::Memory(MemoryBackend::new()),
                public_url: None,
                max_file_size: None,
            }),
        };
        assert!(storage.url("key.jpg").is_err());
    }

    #[tokio::test]
    async fn presigned_url_works_on_memory() {
        let storage = Storage::memory();
        let url = storage
            .presigned_url("key.jpg", std::time::Duration::from_secs(3600))
            .await
            .unwrap();
        assert!(url.contains("key.jpg"));
        assert!(url.contains("expires=3600"));
    }

    #[tokio::test]
    async fn exists_false_for_missing() {
        let storage = Storage::memory();
        assert!(!storage.exists("nonexistent.jpg").await.unwrap());
    }

    #[tokio::test]
    async fn put_rejects_path_traversal() {
        let storage = Storage::memory();
        let input = PutInput {
            data: Bytes::from("data"),
            prefix: "../etc/".into(),
            filename: Some("f.txt".into()),
            content_type: "text/plain".into(),
        };
        assert!(storage.put(&input).await.is_err());
    }

    #[tokio::test]
    async fn put_rejects_absolute_path() {
        let storage = Storage::memory();
        let input = PutInput {
            data: Bytes::from("data"),
            prefix: "/root/".into(),
            filename: Some("f.txt".into()),
            content_type: "text/plain".into(),
        };
        assert!(storage.put(&input).await.is_err());
    }

    #[tokio::test]
    async fn put_rejects_empty_prefix() {
        let storage = Storage::memory();
        let input = PutInput {
            data: Bytes::from("data"),
            prefix: "".into(),
            filename: Some("f.txt".into()),
            content_type: "text/plain".into(),
        };
        assert!(storage.put(&input).await.is_err());
    }

    #[tokio::test]
    async fn put_from_url_memory_backend_returns_error() {
        let storage = Storage::memory();
        let input = PutFromUrlInput {
            url: "https://example.com/file.jpg".into(),
            prefix: "downloads/".into(),
            filename: Some("file.jpg".into()),
        };
        let err = storage.put_from_url(&input).await.err().unwrap();
        assert_eq!(err.status(), http::StatusCode::INTERNAL_SERVER_ERROR);
    }
}
