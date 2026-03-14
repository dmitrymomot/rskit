//! Pluggable storage backends for persisted file uploads.
//!
//! Use [`storage()`] to construct the appropriate backend from
//! [`UploadConfig`](crate::UploadConfig), or instantiate a backend directly:
//!
//! - [`local::LocalStorage`] — writes files to the local filesystem
//!   (requires the `local` feature, enabled by default).
//! - `opendal::OpendalStorage` — delegates to any Apache OpenDAL operator,
//!   including S3-compatible services (requires the `opendal` feature).

mod factory;
#[cfg(feature = "local")]
pub mod local;
#[cfg(feature = "opendal")]
pub mod opendal;
mod types;
pub(crate) mod utils;

pub use factory::storage;
pub use types::{FileStorage, StoredFile};
#[cfg(feature = "opendal")]
pub(crate) use utils::validate_logical_path;
pub(crate) use utils::{ensure_within, generate_filename};
