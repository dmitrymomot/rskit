pub use modo_upload_macros::FromMultipart;

mod config;
mod extractor;
mod file;
pub mod storage;
mod stream;
mod validate;

#[cfg(feature = "opendal")]
pub use config::S3Config;
pub use config::{StorageBackend, UploadConfig};
pub use extractor::MultipartForm;
pub use file::UploadedFile;
pub use storage::{FileStorage, StoredFile, storage};
pub use stream::BufferedUpload;
pub use validate::{gb, kb, mb};

/// Trait for parsing a struct from `multipart/form-data`.
#[async_trait::async_trait]
pub trait FromMultipart: Sized {
    async fn from_multipart(
        multipart: &mut axum::extract::Multipart,
        max_file_size: Option<usize>,
    ) -> Result<Self, modo::Error>;
}

/// Internal helpers exposed for use by generated code. Not public API.
#[doc(hidden)]
pub mod __internal {
    pub use crate::validate::mime_matches;
    pub use async_trait::async_trait;
    pub use axum;
}
