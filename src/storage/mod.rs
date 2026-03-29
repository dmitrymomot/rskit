//! S3-compatible object storage.
//!
//! This module provides [`Storage`], a thin facade over S3-compatible backends
//! (AWS S3, RustFS, MinIO, etc.). Features include upload from bytes or URL,
//! presigned URLs, configurable ACLs, and file-size limits.
//!
//! Requires the `storage` feature flag:
//!
//! ```toml
//! [dependencies]
//! modo = { version = "0.1", features = ["storage"] }
//! ```
//!
//! # Provides
//!
//! | Type | Purpose |
//! |------|---------|
//! | [`Storage`] | Single-bucket handle -- upload, delete, public URL, presigned URL |
//! | [`Buckets`] | Named collection of `Storage` instances for multi-bucket apps |
//! | [`PutInput`] | Input for [`Storage::put()`] / [`Storage::put_with()`] |
//! | [`PutFromUrlInput`] | Input for [`Storage::put_from_url()`] / [`Storage::put_from_url_with()`] |
//! | [`PutOptions`] | Optional headers and ACL override for uploads |
//! | [`Acl`] | Access control: `Private` (default) or `PublicRead` |
//! | [`BucketConfig`] | Deserialisable configuration for one bucket |
//! | [`kb()`] / [`mb()`] / [`gb()`] | Size-unit helpers (bytes conversion) |
//!
//! # Quick start
//!
//! ```
//! use modo::storage::{BucketConfig, Storage, PutInput};
//! # use bytes::Bytes;
//!
//! # fn example() -> modo::Result<()> {
//! let mut config = BucketConfig::default();
//! config.bucket = "my-bucket".into();
//! config.endpoint = "https://s3.amazonaws.com".into();
//! config.access_key = "AKIAIOSFODNN7EXAMPLE".into();
//! config.secret_key = "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".into();
//! config.region = Some("us-east-1".into());
//! config.public_url = Some("https://cdn.example.com".into());
//! config.max_file_size = Some("10mb".into());
//! let storage = Storage::new(&config)?;
//! # Ok(())
//! # }
//! ```
//!
//! # Request signing
//!
//! All requests are signed with AWS Signature Version 4. Both path-style
//! (`https://endpoint/bucket/key`) and virtual-hosted-style
//! (`https://bucket.endpoint/key`) URLs are supported via the
//! `path_style` field in [`BucketConfig`].

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
