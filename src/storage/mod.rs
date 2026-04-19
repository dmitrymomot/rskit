//! # modo::storage
//!
//! S3-compatible object storage (AWS S3, RustFS, MinIO, or any S3-API provider).
//!
//! Provides:
//! - [`Storage`] — single-bucket handle: upload, delete, public URL, presigned URL,
//!   existence check, prefix deletion
//! - [`Buckets`] — named collection of [`Storage`] instances for multi-bucket apps
//! - [`PutInput`] / [`PutFromUrlInput`] — inputs for byte uploads and
//!   fetch-and-upload operations
//! - [`PutOptions`] / [`Acl`] — optional headers (`Content-Disposition`,
//!   `Cache-Control`, content-type override) and `x-amz-acl` override
//! - [`BucketConfig`] — deserialisable per-bucket configuration
//! - [`kb()`] / [`mb()`] / [`gb()`] — size-unit helpers returning `usize` bytes
//!
//! Backends:
//! - Remote S3-compatible backend (default) — used by [`Storage::new`] and
//!   [`Storage::with_client`]. Signs every request with AWS Signature Version 4
//!   and supports both path-style (`https://endpoint/bucket/key`) and
//!   virtual-hosted-style (`https://bucket.endpoint/key`) URLs via
//!   [`BucketConfig::path_style`].
//! - In-memory backend — available via `Storage::memory()` and
//!   `Buckets::memory()` under `#[cfg(test)]` or the `test-helpers` feature.
//!   Does not support [`Storage::put_from_url`].
//!
//! ## Key encoding
//!
//! All operations that route a key into an HTTP request (PUT, DELETE, HEAD,
//! presigned GET) URI-encode the key with AWS rules
//! (`uri_encode(key, encode_slash = false)`) — slashes are preserved so nested
//! prefixes stay as path segments, but every other reserved byte is
//! percent-encoded. Pass raw (unencoded) keys to [`Storage`] methods; do not
//! pre-encode. Keys are validated before signing: empty strings, leading `/`,
//! `..` path segments, and ASCII control characters are rejected with
//! [`Error::bad_request`](crate::Error::bad_request).
//!
//! ## Quick start
//!
//! ```rust,no_run
//! use modo::storage::{BucketConfig, PutInput, Storage};
//!
//! # async fn example() -> modo::Result<()> {
//! let mut config = BucketConfig::default();
//! config.bucket = "my-bucket".into();
//! config.endpoint = "https://s3.amazonaws.com".into();
//! config.access_key = "AKIAIOSFODNN7EXAMPLE".into();
//! config.secret_key = "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".into();
//! config.region = Some("us-east-1".into());
//! config.public_url = Some("https://cdn.example.com".into());
//! config.max_file_size = Some("10mb".into());
//!
//! let storage = Storage::new(&config)?;
//!
//! let mut input = PutInput::new(
//!     bytes::Bytes::from_static(b"file contents"),
//!     "avatars/",
//!     "image/jpeg",
//! );
//! input.filename = Some("photo.jpg".into());
//!
//! let key = storage.put(&input).await?;
//! let public = storage.url(&key)?;
//! # let _ = public;
//! # Ok(())
//! # }
//! ```
//!
//! Use [`Storage::with_client`] to share a [`reqwest::Client`] connection pool
//! across multiple [`Storage`] instances or other modules.

mod backend;
mod bridge;
mod buckets;
mod client;
mod config;
mod facade;
mod fetch;
pub(crate) mod memory;
mod options;
mod path;
mod presign;
mod signing;

pub use buckets::Buckets;
pub use config::BucketConfig;
pub use config::{gb, kb, mb};
pub use facade::{PutFromUrlInput, PutInput, Storage};
pub use options::{Acl, PutOptions};
