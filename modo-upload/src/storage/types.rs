use crate::file::UploadedFile;
use crate::stream::BufferedUpload;
use std::future::Future;
use std::pin::Pin;

/// Metadata returned after a file has been successfully stored.
pub struct StoredFile {
    /// Relative path within the storage backend (e.g. `"avatars/01HXK3Q1A2B3.jpg"`).
    pub path: String,
    /// File size in bytes.
    pub size: u64,
}

/// Trait for persisting uploaded files to a storage backend.
///
/// Both in-memory ([`UploadedFile`]) and chunked ([`BufferedUpload`]) uploads
/// are supported.  Implementors must be `Sync + 'static` so they can be
/// shared across async tasks behind an `Arc<dyn FileStorageDyn>`.
///
/// Use the [`storage()`](crate::storage()) factory function to construct the
/// backend configured by [`UploadConfig`](crate::UploadConfig), or instantiate
/// a concrete backend directly (e.g. [`LocalStorage`](super::local::LocalStorage)).
///
/// Use [`FileStorageDyn`] (object-safe companion) for trait objects:
/// `Arc<dyn FileStorageDyn>`. Any type implementing `FileStorage`
/// automatically implements `FileStorageDyn` via a blanket impl.
#[trait_variant::make(FileStorageSend: Send)]
pub trait FileStorage: Sync + 'static {
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

/// Object-safe companion to [`FileStorage`] for use with `Arc<dyn FileStorageDyn>`.
///
/// This trait is automatically implemented for all types that implement
/// [`FileStorage`] (or `FileStorageSend`).
pub trait FileStorageDyn: Send + Sync + 'static {
    /// Store a buffered in-memory file under `prefix/`.
    ///
    /// A ULID-based unique filename is generated automatically.
    /// Returns the stored path and size on success.
    fn store<'a>(
        &'a self,
        prefix: &'a str,
        file: &'a UploadedFile,
    ) -> Pin<Box<dyn Future<Output = Result<StoredFile, modo::Error>> + Send + 'a>>;

    /// Store a chunked upload under `prefix/`.
    ///
    /// Chunks are consumed from `stream` sequentially.
    /// Returns the stored path and size on success.
    fn store_stream<'a>(
        &'a self,
        prefix: &'a str,
        stream: &'a mut BufferedUpload,
    ) -> Pin<Box<dyn Future<Output = Result<StoredFile, modo::Error>> + Send + 'a>>;

    /// Delete a file by its storage path (as returned by [`store`](Self::store)).
    fn delete<'a>(
        &'a self,
        path: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<(), modo::Error>> + Send + 'a>>;

    /// Return `true` if a file exists at the given storage path.
    fn exists<'a>(
        &'a self,
        path: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<bool, modo::Error>> + Send + 'a>>;
}

impl<T: FileStorageSend> FileStorageDyn for T {
    fn store<'a>(
        &'a self,
        prefix: &'a str,
        file: &'a UploadedFile,
    ) -> Pin<Box<dyn Future<Output = Result<StoredFile, modo::Error>> + Send + 'a>> {
        Box::pin(FileStorageSend::store(self, prefix, file))
    }

    fn store_stream<'a>(
        &'a self,
        prefix: &'a str,
        stream: &'a mut BufferedUpload,
    ) -> Pin<Box<dyn Future<Output = Result<StoredFile, modo::Error>> + Send + 'a>> {
        Box::pin(FileStorageSend::store_stream(self, prefix, stream))
    }

    fn delete<'a>(
        &'a self,
        path: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<(), modo::Error>> + Send + 'a>> {
        Box::pin(FileStorageSend::delete(self, path))
    }

    fn exists<'a>(
        &'a self,
        path: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<bool, modo::Error>> + Send + 'a>> {
        Box::pin(FileStorageSend::exists(self, path))
    }
}
