//! File upload support for modo applications.
//!
//! Provides multipart form parsing, in-memory buffering, pluggable storage
//! backends, and file validation.  The `#[derive(FromMultipart)]`
//! macro generates the boilerplate for mapping multipart fields to struct fields.
//!
//! # Features
//!
//! - `local` (default) — local filesystem storage via [`storage::local::LocalStorage`].
//! - `opendal` — S3-compatible object storage via `storage::opendal::OpendalStorage`
//!   and the `S3Config` configuration type.
//!
//! # Quick start
//!
//! ```rust,ignore
//! use modo_upload::{FromMultipart, MultipartForm, UploadedFile, storage};
//! use modo_upload::UploadConfig;
//!
//! #[derive(FromMultipart)]
//! struct UploadForm {
//!     #[upload(max_size = "5mb", accept = "image/*")]
//!     avatar: UploadedFile,
//!     name: String,
//! }
//!
//! #[modo::handler(POST, "/upload")]
//! async fn upload(form: MultipartForm<UploadForm>) -> modo::JsonResult<()> {
//!     let storage = storage(&UploadConfig::default())?;
//!     let stored = storage.store("avatars", &form.avatar).await?;
//!     println!("stored at {}", stored.path);
//!     Ok(modo::Json(()))
//! }
//! ```

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
///
/// Implement this trait (or derive it with `#[derive(FromMultipart)]`) to
/// describe how multipart fields map to struct fields.  The
/// [`MultipartForm`] extractor calls this automatically during request
/// extraction.
#[async_trait::async_trait]
pub trait FromMultipart: Sized {
    /// Parse `multipart` into `Self`, enforcing `max_file_size` on every file
    /// field when `Some`.
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
