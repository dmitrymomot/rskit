//! # modo::extractor
//!
//! Request extractors for the modo web framework.
//!
//! All sanitizing extractors call [`crate::sanitize::Sanitize::sanitize`] on
//! the deserialized value before returning it, so whitespace trimming and other
//! normalization happen automatically.
//!
//! [`Path`] is re-exported directly from axum and behaves identically.
//!
//! ## Extractors
//!
//! | Extractor | Source | Trait bound |
//! |---|---|---|
//! | [`JsonRequest<T>`] | JSON body | `T: DeserializeOwned + Sanitize` |
//! | [`FormRequest<T>`] | URL-encoded form body | `T: DeserializeOwned + Sanitize` |
//! | [`Query<T>`] | URL query string | `T: DeserializeOwned + Sanitize` |
//! | [`MultipartRequest<T>`] | `multipart/form-data` body | `T: DeserializeOwned + Sanitize` |
//! | [`Path`] | URL path parameters | `T: DeserializeOwned` |
//! | [`Service<T>`] | Service registry | `T: Send + Sync + 'static` |
//! | [`ClientInfo`] | Client IP, user-agent, fingerprint | — |
//!
//! ## Multipart helpers
//!
//! | Type | Purpose |
//! |---|---|
//! | [`UploadedFile`] | Single file extracted from a multipart field |
//! | [`Files`] | Map of field names to uploaded files |
//! | [`UploadValidator`] | Fluent size/content-type validator for [`UploadedFile`] |

mod client_info;
mod form;
mod json;
mod multipart;
mod query;
mod service;
mod upload_validator;

pub use axum::extract::Path;
pub use client_info::ClientInfo;
pub use form::FormRequest;
pub use json::JsonRequest;
pub use multipart::{Files, MultipartRequest, UploadedFile};
pub use query::Query;
pub use service::Service;
pub use upload_validator::UploadValidator;
