//! S3-compatible object storage.
//!
//! This module provides [`Storage`], a thin facade over S3-compatible backends
//! (AWS S3, RustFS, MinIO, etc.). Features include upload from bytes or URL,
//! presigned URLs, configurable ACLs, and file-size limits.
//!
//! Requires the `storage` feature flag.

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
