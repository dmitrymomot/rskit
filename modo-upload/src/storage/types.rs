use crate::file::UploadedFile;
use crate::stream::BufferedUpload;

/// Metadata returned after a file has been successfully stored.
pub struct StoredFile {
    /// Relative path within the storage backend (e.g. `avatars/01HXK3Q1A2B3.jpg`).
    pub path: String,
    /// File size in bytes.
    pub size: u64,
}

/// Trait for persisting uploaded files to a storage backend.
///
/// Both in-memory ([`UploadedFile`]) and chunked ([`BufferedUpload`]) uploads
/// are supported.  Implementors must be `Send + Sync + 'static` so they can be
/// shared across async tasks.
#[async_trait::async_trait]
pub trait FileStorage: Send + Sync + 'static {
    /// Store a buffered in-memory file under `prefix/`.
    ///
    /// A ULID-based unique filename is generated automatically.
    /// Returns the stored path and size on success.
    async fn store(&self, prefix: &str, file: &UploadedFile) -> Result<StoredFile, modo::Error>;

    /// Store a chunked upload under `prefix/`.
    ///
    /// Chunks are consumed from `stream` sequentially.
    /// Returns the stored path and size on success.
    async fn store_stream(
        &self,
        prefix: &str,
        stream: &mut BufferedUpload,
    ) -> Result<StoredFile, modo::Error>;

    /// Delete a file by its storage path (as returned by [`store`](Self::store)).
    async fn delete(&self, path: &str) -> Result<(), modo::Error>;

    /// Return `true` if a file exists at the given storage path.
    async fn exists(&self, path: &str) -> Result<bool, modo::Error>;
}
