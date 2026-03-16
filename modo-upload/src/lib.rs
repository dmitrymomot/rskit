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
//! Define a form struct, register the storage backend as a service, then
//! extract the form in a handler:
//!
//! ```rust,ignore
//! use std::sync::Arc;
//! use modo::{AppConfig, Json, JsonResult, Service};
//! use modo_upload::{FileStorageDyn, FromMultipart, MultipartForm, UploadConfig, UploadedFile, storage};
//! use serde::Deserialize;
//!
//! #[derive(Default, Deserialize)]
//! struct AppSettings {
//!     #[serde(flatten)]
//!     core: AppConfig,
//!     #[serde(default)]
//!     upload: UploadConfig,
//! }
//!
//! #[derive(FromMultipart)]
//! struct UploadForm {
//!     #[upload(max_size = "5mb", accept = "image/*")]
//!     avatar: UploadedFile,
//!     name: String,
//! }
//!
//! #[modo::handler(POST, "/upload")]
//! async fn upload(
//!     file_storage: Service<Arc<dyn FileStorageDyn>>,
//!     form: MultipartForm<UploadForm>,
//! ) -> JsonResult<()> {
//!     let stored = file_storage.store("avatars", &form.avatar).await?;
//!     println!("stored at {}", stored.path);
//!     Ok(Json(()))
//! }
//!
//! #[modo::main]
//! async fn main(
//!     app: modo::app::AppBuilder,
//!     config: AppSettings,
//! ) -> Result<(), Box<dyn std::error::Error>> {
//!     let file_storage = storage(&config.upload)?;
//!     app.config(config.core).service(file_storage).run().await
//! }
//! ```

pub use modo_upload_macros::FromMultipart;

mod config;
mod extractor;
mod file;
mod from_multipart;
pub mod storage;
mod stream;
mod validate;

#[cfg(feature = "opendal")]
pub use config::S3Config;
pub use config::{StorageBackend, UploadConfig};
pub use extractor::MultipartForm;
pub use file::UploadedFile;
pub use from_multipart::FromMultipart;
pub use storage::{FileStorage, FileStorageDyn, FileStorageSend, StoredFile, storage};
pub use stream::BufferedUpload;
pub use validate::{gb, kb, mb};

/// Internal helpers exposed for use by generated code. Not public API.
#[doc(hidden)]
pub mod __internal {
    pub use crate::validate::mime_matches;
    pub use async_trait::async_trait;
    pub use axum;
}
